//! Constrained query-clause evaluation over canonical call facts.
//!
//! Evaluation uses a single clause predicate evaluator shared with the
//! index-based path.  Indexes produce candidate `FactId`s so every candidate
//! follows the same semantic path.  When the occurrence index cannot represent
//! a clause's predicate (e.g. member-call property names that the scope
//! collector did not intern), candidate selection falls back to scanning the
//! fact stream once.
//!
//! Clauses that require argument data retain the same compiled representation
//! as ordinary indexed clauses. A project overlay may strengthen a proven
//! module identity or static string, but unknown and qualified-local identities
//! remain non-matches.
//!
//! Evaluation uses a single clause predicate evaluator shared with the
//! index-based path.  Indexes produce candidate `FactId`s so every candidate
//! follows the same semantic path.  When the occurrence index cannot represent
//! a clause's predicate (e.g. member-call property names that the scope
//! collector did not intern), candidate selection falls back to scanning the
//! fact stream once.

use std::collections::BTreeMap;

use crate::{
    analysis::{
        SymbolPath,
        facts::{CallUnwrap, SemanticFact},
        matching::{
            CallArgInfo, ClassificationEvidence, FactPayload, FactStream, LinkedModuleIdentity,
            ModuleExportKey, ModuleIdentityMap, Occurrence, SymbolCallProvenance,
            push_owned_evidence,
        },
        name::NameTable,
        syntax::SymbolMemberProvenance,
        value::{NamePath, ValueId},
    },
    api::compiler::rule::{
        EventPredicate, IdentityConstraint, QueryClause, QueryConstraint, SubjectConstraint,
    },
};

/// Evaluate constrained clauses once over call facts.
///
/// Every candidate fact is checked by the same [`fact_matches_clause`]
/// predicate, so constraint evaluation follows one semantic path regardless
/// of how candidates were collected.
pub(in crate::analysis) fn compute_constrained_evidence_from_stream_with_overlay(
    stream: &FactStream,
    clauses: &[(usize, &QueryClause)],
    evidence: &mut [Vec<ClassificationEvidence>],
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<&BTreeMap<ValueId, LinkedModuleIdentity>>,
) {
    let Some(names) = stream.names() else {
        return;
    };
    for (rule_index, clause) in clauses {
        let candidates: Vec<Occurrence> = stream
            .facts()
            .iter()
            .filter(|fact| fact_matches_clause(fact, clause, names, identities, result_identities))
            .map(|fact| Occurrence::new(fact.id, fact.span))
            .collect();

        if candidates.is_empty() {
            continue;
        }

        for occurrence in candidates {
            push_owned_evidence(
                &mut evidence[*rule_index],
                clause.evidence.kind,
                clause.evidence.symbol.clone(),
                std::iter::once(occurrence),
            );
        }
    }
}

fn argument_with_overlay(
    argument: &CallArgInfo,
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<&BTreeMap<ValueId, LinkedModuleIdentity>>,
) -> CallArgInfo {
    let mut argument = argument.clone();
    if let Some(result_identities) = result_identities
        && let Some(identity) = result_identities.get(&argument.value)
    {
        apply_identity_to_argument(&mut argument, identity);
    }
    if let Some(identities) = identities
        && let SymbolCallProvenance::ModuleExport { module, export } = &argument.provenance
        && let Some(identity) =
            identities.get(&ModuleExportKey::new(module.clone(), export.clone()))
    {
        apply_identity_to_argument(&mut argument, identity);
    }
    argument
}

fn apply_identity_to_argument(argument: &mut CallArgInfo, identity: &LinkedModuleIdentity) {
    if let LinkedModuleIdentity::StaticString { value } = identity {
        argument.static_string = Some(value.clone());
    }
    if let LinkedModuleIdentity::External { module, export } = identity {
        argument.provenance = SymbolCallProvenance::ModuleExport {
            module: module.clone(),
            export: export.clone(),
        };
    }
}

/// Shared predicate: does a fact match a clause's event, identity, subject,
/// and argument constraints, using string-based comparison (not NamePath)?
///
/// This is the single predicate evaluator that the constrained path uses for
/// candidate selection.  The unconstrained index path has an equivalent but
/// separate implementation because its keys are NamePaths; the subject-matter
/// logic (event, identity, subject) is intentionally kept the same.
fn fact_matches_clause(
    fact: &SemanticFact,
    clause: &QueryClause,
    names: &NameTable,
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<&BTreeMap<ValueId, LinkedModuleIdentity>>,
) -> bool {
    let FactPayload::Call {
        callee,
        syntactic_chain,
        rooted_chain,
        returned_member,
        instance_class,
        call_provenance,
        callee_name,
        args,
        unwrap,
        ..
    } = &fact.payload
    else {
        return false;
    };
    let callee_name: Option<smol_str::SmolStr> =
        callee_name.and_then(|id| names.resolve(id).map(Into::into));
    let call_provenance =
        overlaid_call_provenance(call_provenance, *callee, identities, result_identities);

    match &clause.event {
        EventPredicate::Call => {
            if !matches!(clause.subject, SubjectConstraint::Direct) {
                return false;
            }
            if !call_identity_matches(
                clause,
                &call_provenance,
                callee_name.as_ref(),
                syntactic_chain.as_ref(),
            ) {
                return false;
            }
            check_constrained_args(
                clause,
                args,
                unwrap.as_deref(),
                identities,
                result_identities,
                names,
            )
        }
        EventPredicate::MemberCall { member } => {
            if !member_subject_matches(
                clause,
                member,
                returned_member.as_ref(),
                instance_class.as_ref(),
                names,
            ) {
                return false;
            }
            if !member_identity_matches(
                clause,
                member,
                syntactic_chain.as_ref(),
                rooted_chain.as_ref(),
                fact,
                names,
            ) {
                return false;
            }
            let linked_args: Vec<CallArgInfo> = args
                .iter()
                .map(|a| argument_with_overlay(a, identities, result_identities))
                .collect();
            constraints_match(&clause.constraints, &linked_args, names)
        }
        _ => false,
    }
}

/// Apply identity and result overlay to a call's raw provenance.
fn overlaid_call_provenance(
    raw: &SymbolCallProvenance,
    callee: ValueId,
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<&BTreeMap<ValueId, LinkedModuleIdentity>>,
) -> SymbolCallProvenance {
    // Result identities are set by flow analysis and take priority.
    if let Some(result_identities) = result_identities
        && let Some(identity) = result_identities.get(&callee)
    {
        return match identity {
            LinkedModuleIdentity::External { module, export } => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.clone(),
                }
            }
            LinkedModuleIdentity::Global { name } => {
                SymbolCallProvenance::Global { name: name.clone() }
            }
            _ => raw.clone(),
        };
    }
    // Project-level identity map upgrades module-export provenances.
    if let SymbolCallProvenance::ModuleExport { module, export } = raw
        && let Some(identities) = identities
        && let Some(identity) =
            identities.get(&ModuleExportKey::new(module.clone(), export.clone()))
    {
        return match identity {
            LinkedModuleIdentity::External { module, export } => {
                SymbolCallProvenance::ModuleExport {
                    module: module.clone(),
                    export: export.clone(),
                }
            }
            LinkedModuleIdentity::Global { name } => {
                SymbolCallProvenance::Global { name: name.clone() }
            }
            _ => raw.clone(),
        };
    }
    raw.clone()
}

fn call_identity_matches(
    clause: &QueryClause,
    call_provenance: &SymbolCallProvenance,
    callee_name: Option<&smol_str::SmolStr>,
    syntactic_chain: Option<&SymbolPath>,
) -> bool {
    match &clause.identity {
        IdentityConstraint::Any { name, .. } => {
            callee_name.is_some_and(|found| *found == *name)
                || syntactic_chain.is_some_and(|chain| chain.eq_chain(name))
        }
        IdentityConstraint::Global { name, .. } => {
            matches!(call_provenance, SymbolCallProvenance::Global { name: found } if found == name)
        }
        IdentityConstraint::ModuleExport { module, export } => {
            matches!(call_provenance, SymbolCallProvenance::ModuleExport {
                module: found_module, export: found_export
            } if found_module == module && found_export == export)
        }
        IdentityConstraint::PackageModuleExport { module, export } => {
            matches!(call_provenance, SymbolCallProvenance::ModuleExport {
                module: found_module, export: found_export
            } if module.matches(found_module) && found_export == export)
        }
        _ => false,
    }
}

fn member_subject_matches(
    clause: &QueryClause,
    member: &SymbolPath,
    returned_member: Option<&(NamePath, NamePath)>,
    instance_class: Option<&(smol_str::SmolStr, smol_str::SmolStr)>,
    names: &NameTable,
) -> bool {
    match &clause.subject {
        SubjectConstraint::Direct => true,
        SubjectConstraint::ReturnedFrom { producer } => {
            returned_member.is_some_and(|(source, found)| {
                NamePath::from_symbol_path(member, names).is_some_and(|member| found == &member)
                    && source
                        .to_symbol_path(names)
                        .is_some_and(|source| producer.exact_root_matches(&source))
            })
        }
        SubjectConstraint::InstanceOf { constructor } => instance_class
            .is_some_and(|(module, export)| constructor.identity_module_matches(module, export)),
    }
}

fn member_identity_matches(
    clause: &QueryClause,
    member: &SymbolPath,
    syntactic_chain: Option<&SymbolPath>,
    rooted_chain: Option<&NamePath>,
    fact: &SemanticFact,
    names: &NameTable,
) -> bool {
    let FactPayload::Call { module_member, .. } = &fact.payload else {
        return false;
    };
    match (&clause.identity, &clause.subject) {
        (IdentityConstraint::Any { name, .. }, SubjectConstraint::Direct) => {
            member.eq_chain(name)
                && (syntactic_chain.is_some_and(|chain| chain == member)
                    || rooted_chain.is_some_and(|chain| {
                        NamePath::from_symbol_path(member, names)
                            .is_some_and(|member| chain == &member)
                    }))
        }
        (IdentityConstraint::Rooted { path }, SubjectConstraint::Direct) => rooted_chain
            .is_some_and(|chain| {
                let canonical = chain.without_this_prefix(names);
                NamePath::from_symbol_path(path, names).is_some_and(|path| canonical == path)
                    && NamePath::from_symbol_path(member, names)
                        .is_some_and(|member| canonical == member)
            }),
        (IdentityConstraint::Rooted { path }, SubjectConstraint::ReturnedFrom { .. }) => {
            let FactPayload::Call {
                returned_member, ..
            } = &fact.payload
            else {
                return false;
            };
            returned_member.as_ref().is_some_and(|(source, found)| {
                NamePath::from_symbol_path(path, names).is_some_and(|path| source == &path)
                    && NamePath::from_symbol_path(member, names)
                        .is_some_and(|member| found == &member)
            })
        }
        (
            IdentityConstraint::ModuleExport { module, export },
            SubjectConstraint::InstanceOf { .. },
        ) => {
            let FactPayload::Call { instance_class, .. } = &fact.payload else {
                return false;
            };
            instance_class
                .as_ref()
                .is_some_and(|(found_module, found_export)| {
                    found_module == module && found_export == export
                })
                && syntactic_chain.and_then(|s| s.last_segment()) == member.last_segment()
        }
        (
            IdentityConstraint::PackageModuleExport { module, export },
            SubjectConstraint::InstanceOf { .. },
        ) => {
            let FactPayload::Call { instance_class, .. } = &fact.payload else {
                return false;
            };
            instance_class
                .as_ref()
                .is_some_and(|(found_module, found_export)| {
                    module.matches(found_module) && found_export == export
                })
                && syntactic_chain.and_then(|s| s.last_segment()) == member.last_segment()
        }
        (IdentityConstraint::ModuleNamespace { module }, SubjectConstraint::Direct) => {
            matches!(
                module_member,
                Some(SymbolMemberProvenance::ModuleNamespace {
                    module: found_module, member: found_member
                }) if found_module == module && member.eq_chain(found_member)
            )
        }
        (IdentityConstraint::PackageModuleNamespace { module }, SubjectConstraint::Direct) => {
            matches!(
                module_member,
                Some(SymbolMemberProvenance::ModuleNamespace {
                    module: found_module, member: found_member
                }) if module.matches(found_module) && member.eq_chain(found_member)
            )
        }
        _ => false,
    }
}

fn check_constrained_args(
    clause: &QueryClause,
    args: &[CallArgInfo],
    unwrap: Option<&CallUnwrap>,
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<&BTreeMap<ValueId, LinkedModuleIdentity>>,
    names: &NameTable,
) -> bool {
    let linked_args: Vec<CallArgInfo> = args
        .iter()
        .map(|a| argument_with_overlay(a, identities, result_identities))
        .collect();

    // For unwrapped calls (.call()/.apply()), check effective args.
    unwrap.map_or_else(
        || constraints_match(&clause.constraints, &linked_args, names),
        |unwrap| {
            let linked_effective: Vec<CallArgInfo> = unwrap
                .effective_args
                .iter()
                .map(|a| argument_with_overlay(a, identities, result_identities))
                .collect();
            constraints_match(&clause.constraints, &linked_effective, names)
        },
    )
}

fn constraints_match(
    constraints: &[QueryConstraint],
    args: &[CallArgInfo],
    names: &NameTable,
) -> bool {
    constraints.iter().all(|constraint| match constraint {
        QueryConstraint::Argument(argument) => args
            .get(argument.index)
            .is_some_and(|value| argument.matcher.matches(value, names)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Environment,
        analysis::{
            facts::{CallArgInfo, FactStream, build::build_test_stream},
            lowering::SpanNormalizer,
            matching::{LinkedModuleIdentity, OccurrenceIndexes},
            resolution::Resolver,
            syntax::SymbolCallProvenance,
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

    fn stream(source: &str, environment: &Environment) -> FactStream {
        let parsed = crate::parse(source, "constrained.js").unwrap();
        let coordinates = SpanNormalizer::new(parsed.source_start, source);
        let resolver =
            Resolver::collect_with_environment(&parsed.program, environment, coordinates);
        build_test_stream(&parsed.program, &resolver)
    }

    fn build_index(stream: &FactStream) -> OccurrenceIndexes {
        let mut index = OccurrenceIndexes::default();
        if stream.is_valid() {
            index.build_from_stream(stream);
            index.normalize_occurrences();
        }
        index
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
            &Environment::default(),
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
        let _index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
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
        let mut environment = Environment::default();
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
        let _index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
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

        let stream = stream("fetch('/api');\nfetch('/api');", &Environment::default());
        let _index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
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
        let mut normalized = std::mem::take(&mut evidence[0]);
        crate::analysis::evidence::normalize_evidence(&mut normalized, usize::MAX);
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
    fn argument_overlay_applies_static_string_from_identity_map() {
        let mut identities = ModuleIdentityMap::new();
        identities.insert(
            ModuleExportKey::new("api", "request"),
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
    }
}
