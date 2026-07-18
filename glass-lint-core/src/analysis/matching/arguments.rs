//! Constrained query-clause evaluation over canonical call facts.
//!
//! Clauses that require argument data retain the same compiled representation
//! as ordinary indexed clauses. A project overlay may strengthen a proven
//! module identity or static string, but unknown and qualified-local identities
//! remain non-matches.

use super::{
    CallArgInfo, ClassificationEvidence, FactPayload, FactStream, ModuleExportKey,
    ModuleIdentityMap, Occurrence, OccurrenceIndexes, SymbolCallProvenance, SymbolMemberProvenance,
    push_owned_evidence,
};
use crate::{
    analysis::{SymbolPath, syntax::UnknownReason},
    api::compiler::rule::{
        EventPredicate, IdentityConstraint, QueryClause, QueryConstraint, SubjectConstraint,
    },
};

impl OccurrenceIndexes {
    /// Evaluate selected constrained clauses once over canonical call facts.
    pub(in crate::analysis) fn compute_constrained_evidence_from_stream_with_overlay(
        stream: &FactStream,
        clauses: &[(usize, &QueryClause)],
        evidence: &mut [Vec<ClassificationEvidence>],
        identities: Option<&ModuleIdentityMap>,
        result_identities: Option<
            &std::collections::BTreeMap<super::super::value::ValueId, super::LinkedModuleIdentity>,
        >,
    ) {
        for fact in stream.facts() {
            if let FactPayload::Call {
                callee,
                callee_name,
                call_provenance,
                module_member,
                args,
                unwrap,
                ..
            } = &fact.payload
            {
                let linked_call_provenance = call_provenance_with_overlay(
                    call_provenance,
                    identities,
                    result_identities,
                    *callee,
                );
                let linked_member_provenance =
                    module_member_with_overlay(module_member.as_ref(), identities);
                let linked_args = args
                    .iter()
                    .map(|argument| argument_with_overlay(argument, identities, result_identities))
                    .collect::<Vec<_>>();
                let (effective_args, effective_name, effective_chain) = unwrap
                    .as_ref()
                    .map_or((&linked_args, callee_name.as_deref(), None), |unwrapped| {
                        (&unwrapped.effective_args, None, Some(&unwrapped.chain))
                    });
                for (rule_index, clause) in clauses {
                    let matches = match clause.event {
                        EventPredicate::Call => matches_call_clause(
                            clause,
                            effective_name,
                            effective_chain,
                            &linked_call_provenance,
                            effective_args,
                        ),
                        EventPredicate::MemberCall { .. } => matches_member_clause(
                            clause,
                            fact,
                            linked_member_provenance.as_ref(),
                            &linked_args,
                        ),
                        EventPredicate::Construct
                        | EventPredicate::MemberRead { .. }
                        | EventPredicate::ClassReference
                        | EventPredicate::Import
                        | EventPredicate::StringReference => false,
                    };
                    if !matches {
                        continue;
                    }
                    push_owned_evidence(
                        &mut evidence[*rule_index],
                        clause.evidence.kind,
                        clause.evidence.symbol.clone(),
                        Some(vec![Occurrence::new(fact.id, fact.span)]),
                    );
                }
            }
        }
    }
}

fn argument_with_overlay(
    argument: &CallArgInfo,
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<
        &std::collections::BTreeMap<super::super::value::ValueId, super::LinkedModuleIdentity>,
    >,
) -> CallArgInfo {
    let mut argument = argument.clone();
    if let Some(result_identities) = result_identities
        && let Some(identity) = result_identities.get(&argument.value)
    {
        apply_identity_to_argument(&mut argument, identity);
    }
    if let Some(identities) = identities
        && let SymbolCallProvenance::ModuleExport { module, export } = &argument.provenance
        && let Some(identity) = identities.get(&ModuleExportKey::new(module, export))
    {
        apply_identity_to_argument(&mut argument, identity);
    }
    argument
}

fn apply_identity_to_argument(argument: &mut CallArgInfo, identity: &super::LinkedModuleIdentity) {
    if let super::LinkedModuleIdentity::StaticString { value } = identity {
        argument.static_string = Some(value.clone());
    }
    if let super::LinkedModuleIdentity::External { module, export } = identity {
        argument.provenance = SymbolCallProvenance::ModuleExport {
            module: module.clone(),
            export: export.clone(),
        };
    }
}

fn call_provenance_with_overlay(
    provenance: &SymbolCallProvenance,
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<
        &std::collections::BTreeMap<super::super::value::ValueId, super::LinkedModuleIdentity>,
    >,
    callee: super::super::value::ValueId,
) -> SymbolCallProvenance {
    if !provenance.knowledge().is_known() {
        return provenance.clone();
    }
    if let Some(result_identities) = result_identities
        && matches!(provenance, SymbolCallProvenance::Local)
        && let Some(super::LinkedModuleIdentity::External { module, export }) =
            result_identities.get(&callee)
    {
        return SymbolCallProvenance::ModuleExport {
            module: module.clone(),
            export: export.clone(),
        };
    }
    let Some(identities) = identities else {
        return provenance.clone();
    };
    let SymbolCallProvenance::ModuleExport { module, export } = provenance else {
        return provenance.clone();
    };
    let exact_identity = identities.get(&ModuleExportKey::new(module, export));
    let identity = exact_identity.or_else(|| identities.get(&ModuleExportKey::wildcard(module)));
    match identity {
        Some(super::LinkedModuleIdentity::External {
            module: linked_module,
            export: linked_export,
        }) => SymbolCallProvenance::ModuleExport {
            module: linked_module.clone(),
            export: exact_identity.map_or_else(|| export.clone(), |_| linked_export.clone()),
        },
        Some(super::LinkedModuleIdentity::Global { name }) => {
            SymbolCallProvenance::Global { name: name.clone() }
        }
        Some(
            super::LinkedModuleIdentity::Qualified { .. }
            | super::LinkedModuleIdentity::StaticString { .. },
        ) => SymbolCallProvenance::Unknown(UnknownReason::Unresolved),
        Some(super::LinkedModuleIdentity::Unknown) => SymbolCallProvenance::Ambiguous,
        None => provenance.clone(),
    }
}

fn module_member_with_overlay(
    provenance: Option<&SymbolMemberProvenance>,
    identities: Option<&ModuleIdentityMap>,
) -> Option<SymbolMemberProvenance> {
    let Some(SymbolMemberProvenance::ModuleNamespace { module, member }) = provenance else {
        return provenance.cloned();
    };
    let Some(identities) = identities else {
        return provenance.cloned();
    };
    let identity = identities
        .get(&ModuleExportKey::new(module, member))
        .or_else(|| identities.get(&ModuleExportKey::wildcard(module)));
    match identity {
        Some(super::LinkedModuleIdentity::External { module, .. }) => {
            Some(SymbolMemberProvenance::ModuleNamespace {
                module: module.clone(),
                member: member.clone(),
            })
        }
        Some(
            super::LinkedModuleIdentity::Global { .. }
            | super::LinkedModuleIdentity::Qualified { .. }
            | super::LinkedModuleIdentity::StaticString { .. }
            | super::LinkedModuleIdentity::Unknown,
        ) => None,
        None => provenance.cloned(),
    }
}

fn matches_call_clause(
    clause: &QueryClause,
    callee_name: Option<&str>,
    callee_chain: Option<&crate::analysis::SymbolPath>,
    call_provenance: &SymbolCallProvenance,
    args: &[CallArgInfo],
) -> bool {
    if !matches!(clause.subject, SubjectConstraint::Direct) {
        return false;
    }
    let identity_matches = match &clause.identity {
        IdentityConstraint::Any { name, .. } => {
            callee_name == Some(name.as_str())
                || callee_chain.is_some_and(|chain| chain.eq_chain(name))
        }
        IdentityConstraint::Global { name, .. } => matches!(
            call_provenance,
            SymbolCallProvenance::Global { name: found } if found == name
        ),
        IdentityConstraint::ModuleExport { module, export } => matches!(
            call_provenance,
            SymbolCallProvenance::ModuleExport {
                module: found_module,
                export: found_export
            } if found_module == module && found_export == export
        ),
        IdentityConstraint::PackageModuleExport { module, export } => matches!(
            call_provenance,
            SymbolCallProvenance::ModuleExport {
                module: found_module,
                export: found_export
            } if module.matches(found_module) && found_export == export
        ),
        IdentityConstraint::ModuleNamespace { .. }
        | IdentityConstraint::PackageModuleNamespace { .. }
        | IdentityConstraint::Rooted { .. }
        | IdentityConstraint::LiteralString { .. }
        | IdentityConstraint::PackageSpecifier { .. } => false,
    };
    identity_matches && constraints_match(&clause.constraints, args)
}

fn matches_member_clause(
    clause: &QueryClause,
    fact: &super::super::facts::SemanticFact,
    module_member: Option<&SymbolMemberProvenance>,
    args: &[CallArgInfo],
) -> bool {
    let FactPayload::Call {
        syntactic_chain,
        rooted_chain,
        returned_member,
        instance_class,
        ..
    } = &fact.payload
    else {
        return false;
    };
    let EventPredicate::MemberCall { member } = &clause.event else {
        return false;
    };
    let subject_matches = match &clause.subject {
        SubjectConstraint::Direct => true,
        SubjectConstraint::ReturnedFrom { producer } => returned_member
            .as_ref()
            .is_some_and(|(source, found)| found == member && producer.exact_root_matches(source)),
        SubjectConstraint::InstanceOf { constructor } => instance_class
            .as_ref()
            .is_some_and(|(module, export)| constructor.identity_module_matches(module, export)),
    };
    if !subject_matches {
        return false;
    }
    let identity_matches = match (&clause.identity, &clause.subject) {
        (IdentityConstraint::Any { name, .. }, SubjectConstraint::Direct) => {
            member.eq_chain(name)
                && (syntactic_chain
                    .as_ref()
                    .is_some_and(|chain| chain == member)
                    || rooted_chain.as_ref().is_some_and(|chain| chain == member))
        }
        (IdentityConstraint::Rooted { path }, SubjectConstraint::Direct) => {
            rooted_chain.as_ref().is_some_and(|chain| {
                let canonical = chain.without_this_prefix();
                canonical == *path && canonical == *member
            })
        }
        (IdentityConstraint::ModuleNamespace { module }, SubjectConstraint::Direct) => matches!(
            module_member,
            Some(SymbolMemberProvenance::ModuleNamespace {
                module: found_module,
                member: found_member
            }) if found_module == module && member.eq_chain(found_member)
        ),
        (IdentityConstraint::PackageModuleNamespace { module }, SubjectConstraint::Direct) => {
            matches!(
                module_member,
                Some(SymbolMemberProvenance::ModuleNamespace {
                    module: found_module,
                    member: found_member
                }) if module.matches(found_module) && member.eq_chain(found_member)
            )
        }
        (IdentityConstraint::Rooted { path }, SubjectConstraint::ReturnedFrom { .. }) => {
            returned_member
                .as_ref()
                .is_some_and(|(source, found)| source == path && found == member)
        }
        (
            IdentityConstraint::ModuleExport { module, export },
            SubjectConstraint::InstanceOf { .. },
        ) => {
            instance_class
                .as_ref()
                .is_some_and(|(found_module, found_export)| {
                    found_module == module && found_export == export
                })
                && syntactic_chain.as_ref().and_then(SymbolPath::last_segment)
                    == member.last_segment()
        }
        (
            IdentityConstraint::PackageModuleExport { module, export },
            SubjectConstraint::InstanceOf { .. },
        ) => {
            instance_class
                .as_ref()
                .is_some_and(|(found_module, found_export)| {
                    module.matches(found_module) && found_export == export
                })
                && syntactic_chain.as_ref().and_then(SymbolPath::last_segment)
                    == member.last_segment()
        }
        _ => false,
    };
    identity_matches && constraints_match(&clause.constraints, args)
}

fn constraints_match(constraints: &[QueryConstraint], args: &[CallArgInfo]) -> bool {
    constraints.iter().all(|constraint| match constraint {
        QueryConstraint::Argument(argument) => args
            .get(argument.index)
            .is_some_and(|value| argument.matcher.matches(value)),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        OccurrenceIndexes, argument_with_overlay, call_provenance_with_overlay,
        module_member_with_overlay,
    };
    use crate::{
        analysis::{
            facts::{CallArgInfo, FactStream, build::build_test_stream},
            lowering::SpanNormalizer,
            matching::LinkedModuleIdentity,
            resolution::Resolver,
            syntax::{SymbolCallProvenance, SymbolMemberProvenance, UnknownReason},
            value::{PathId, ValueId},
        },
        api::{
            classification::MatchKind,
            compiler::{
                CompiledMatcherPlan,
                rule::{
                    EventPredicate, EvidenceDescriptor, IdentityConstraint, IdentityStrength,
                    QueryClause, QueryConstraint, SubjectConstraint,
                },
            },
            rule::{ArgumentConstraint, CallMatcher, Matcher, MatcherSet, ValueMatcher},
        },
    };

    fn stream(source: &str, environment: &crate::Environment) -> FactStream {
        let parsed = crate::parse(source, "constrained.js").unwrap();
        let coordinates = SpanNormalizer::new(parsed.source_start, source);
        let resolver =
            Resolver::collect_with_environment(&parsed.program, environment, coordinates);
        build_test_stream(&parsed.program, &resolver)
    }

    fn exact_argument(value: &str) -> Box<[QueryConstraint]> {
        Box::new([QueryConstraint::Argument(ArgumentConstraint {
            index: 0,
            matcher: ValueMatcher::static_string().equals(value).into(),
        })])
    }

    fn clause(
        identity: IdentityConstraint,
        event: EventPredicate,
        subject: SubjectConstraint,
        symbol: &str,
    ) -> QueryClause {
        QueryClause {
            identity,
            event,
            subject,
            constraints: exact_argument("/api"),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: symbol.into(),
            },
        }
    }

    #[test]
    fn constrained_calls_and_members_execute_once() {
        let stream = stream(
            "fetch('/api'); client.open('/api');",
            &crate::Environment::default(),
        );
        let call = clause(
            IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            EventPredicate::Call,
            SubjectConstraint::Direct,
            "fetch",
        );
        let member = clause(
            IdentityConstraint::Any {
                name: "client.open".into(),
                strength: IdentityStrength::Heuristic,
            },
            EventPredicate::MemberCall {
                member: "client.open".into(),
            },
            SubjectConstraint::Direct,
            "client.open",
        );
        let mut evidence = vec![Vec::new()];
        OccurrenceIndexes::compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &[(0, &call), (0, &member)],
            &mut evidence,
            None,
            None,
        );
        assert_eq!(evidence[0].len(), 2);
        assert!(evidence[0].iter().all(|item| item.count == 1));
        assert_ne!(
            evidence[0][0].occurrences[0].fact,
            evidence[0][1].occurrences[0].fact
        );
    }

    #[test]
    fn constraints_compose_with_non_direct_subject() {
        let mut environment = crate::Environment::default();
        environment.add_global_object("app").unwrap();
        let stream = stream(
            "import { Client } from 'pkg';\nconst leaf = app.workspace.getLeaf();\nleaf.openFile('/api');\nclass Child extends Client { sendNow() { this.send('/api'); } }",
            &environment,
        );
        let returned = clause(
            IdentityConstraint::Rooted {
                path: "app.workspace.getLeaf".into(),
            },
            EventPredicate::MemberCall {
                member: "openFile".into(),
            },
            SubjectConstraint::ReturnedFrom {
                producer: Box::new(IdentityConstraint::Rooted {
                    path: "app.workspace.getLeaf".into(),
                }),
            },
            "app.workspace.getLeaf.openFile",
        );
        let constructor = IdentityConstraint::ModuleExport {
            module: "pkg".into(),
            export: "Client".into(),
        };
        let instance = clause(
            constructor.clone(),
            EventPredicate::MemberCall {
                member: "send".into(),
            },
            SubjectConstraint::InstanceOf {
                constructor: Box::new(constructor),
            },
            "pkg:Client.send",
        );
        let mut evidence = vec![Vec::new()];
        OccurrenceIndexes::compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &[(0, &returned), (0, &instance)],
            &mut evidence,
            None,
            None,
        );
        assert_eq!(
            evidence[0]
                .iter()
                .map(|item| item.symbol.as_str())
                .collect::<Vec<_>>(),
            ["app.workspace.getLeaf.openFile", "pkg:Client.send"]
        );
    }

    #[test]
    fn constrained_clause_evidence_is_source_ordered_and_deduplicated() {
        let declaration =
            Matcher::from(CallMatcher::heuristic("fetch").arg_static_strings(0, ["/api"]));
        let matcher = MatcherSet::from_matchers(vec![declaration.clone(), declaration]);
        let plan = CompiledMatcherPlan::compile(&matcher);
        let clauses = plan.query().clauses();
        assert_eq!(clauses.len(), 1, "equivalent clauses compile once");

        let stream = stream(
            "fetch('/api');\nfetch('/api');",
            &crate::Environment::default(),
        );
        let mut evidence = vec![Vec::new()];
        OccurrenceIndexes::compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &[(0, &clauses[0])],
            &mut evidence,
            None,
            None,
        );
        assert_eq!(evidence[0].len(), 2);
        assert!(
            evidence[0]
                .iter()
                .all(|item| !item.occurrences[0].span.is_empty())
        );
        evidence[0].reverse();
        let normalized = crate::analysis::evidence::AnnotatedEvidence::from_evidence(
            std::mem::take(&mut evidence[0]),
            usize::MAX,
        )
        .into_evidence();
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].count, 2);
        assert_eq!(normalized[0].occurrences.len(), 2);
        assert!(
            normalized[0]
                .occurrences
                .windows(2)
                .all(|pair| { (pair[0].span, pair[0].fact) < (pair[1].span, pair[1].fact) })
        );
    }

    #[test]
    fn evidence_overlay_preserves_unknown_and_ambiguous_boundaries() {
        let mut identities = super::ModuleIdentityMap::new();
        identities.insert(
            super::ModuleExportKey::new("api", "request"),
            LinkedModuleIdentity::Global {
                name: "fetch".into(),
            },
        );
        assert_eq!(
            call_provenance_with_overlay(
                &SymbolCallProvenance::Unknown(UnknownReason::Cycle),
                Some(&identities),
                None,
                ValueId::UNKNOWN,
            ),
            SymbolCallProvenance::Unknown(UnknownReason::Cycle)
        );

        let module_export = SymbolCallProvenance::ModuleExport {
            module: "api".into(),
            export: "request".into(),
        };
        assert_eq!(
            call_provenance_with_overlay(&module_export, Some(&identities), None, ValueId::UNKNOWN,),
            SymbolCallProvenance::Global {
                name: "fetch".into()
            }
        );

        identities.insert(
            super::ModuleExportKey::new("api", "request"),
            LinkedModuleIdentity::Unknown,
        );
        assert_eq!(
            call_provenance_with_overlay(&module_export, Some(&identities), None, ValueId::UNKNOWN,),
            SymbolCallProvenance::Ambiguous
        );
    }

    #[test]
    fn argument_and_member_overlays_cover_all_linked_identity_outcomes() {
        let mut identities = super::ModuleIdentityMap::new();
        identities.insert(
            super::ModuleExportKey::new("api", "request"),
            LinkedModuleIdentity::StaticString {
                value: "https://example.test".into(),
            },
        );
        let argument = CallArgInfo {
            value: ValueId(7),
            base_value: ValueId::UNKNOWN,
            base_path: PathId::EMPTY,
            static_string: None,
            object_keys: None,
            property_strings: Vec::new(),
            rooted_chain: None,
            projections: Vec::new(),
            spread: false,
            provenance: SymbolCallProvenance::ModuleExport {
                module: "api".into(),
                export: "request".into(),
            },
        };
        assert_eq!(
            argument_with_overlay(&argument, Some(&identities), None).static_string,
            Some("https://example.test".into())
        );

        let member = Some(SymbolMemberProvenance::ModuleNamespace {
            module: "api".into(),
            member: "request".into(),
        });
        identities.insert(
            super::ModuleExportKey::new("api", "request"),
            LinkedModuleIdentity::External {
                module: "platform".into(),
                export: "request".into(),
            },
        );
        assert!(
            module_member_with_overlay(Some(&member.clone().unwrap()), Some(&identities)).is_some()
        );
        identities.insert(
            super::ModuleExportKey::new("api", "request"),
            LinkedModuleIdentity::Unknown,
        );
        assert!(module_member_with_overlay(member.as_ref(), Some(&identities)).is_none());
    }
}
