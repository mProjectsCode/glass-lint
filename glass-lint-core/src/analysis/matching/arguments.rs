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

use std::collections::BTreeMap;

use smol_str::SmolStr;

use crate::{
    analysis::{
        SymbolPath,
        facts::{ArgumentView, CallUnwrap, SemanticFact},
        matching::{
            CallArgInfo, ClassificationEvidence, FactPayload, FactStream, LinkedModuleIdentity,
            ModuleIdentityMap, ModuleOccurrenceOverlay, Occurrence, OccurrenceIndexes,
            SymbolCallProvenance,
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

struct MatcherEvaluator<'a> {
    names: &'a NameTable,
    identities: Option<&'a ModuleIdentityMap>,
    result_identities: Option<&'a BTreeMap<ValueId, LinkedModuleIdentity>>,
}

impl<'a> MatcherEvaluator<'a> {
    fn new(
        names: &'a NameTable,
        identities: Option<&'a ModuleIdentityMap>,
        result_identities: Option<&'a BTreeMap<ValueId, LinkedModuleIdentity>>,
    ) -> Self {
        Self {
            names,
            identities,
            result_identities,
        }
    }

    /// Look up the resolved identity for a module-export provenance.
    ///
    /// Look up module provenance without constructing a temporary owned key.
    fn lookup_identity(
        &self,
        provenance: &SymbolCallProvenance,
    ) -> Option<&LinkedModuleIdentity> {
        let (module, export) = provenance.module_export_parts()?;
        self.identities?.get_parts(module, export)
    }

    fn argument_with_overlay<'b>(&'b self, argument: &'b CallArgInfo) -> ArgumentView<'b> {
        let mut view = ArgumentView::new(argument);
        if let Some(result_identities) = self.result_identities
            && let Some(value) = result_identities
                .get(&argument.value)
                .and_then(LinkedModuleIdentity::static_string_value)
        {
            view = view.with_static_string(value);
        }
        if let Some(identity) = self.lookup_identity(&argument.provenance)
            && let Some(value) = identity.static_string_value()
        {
            view = view.with_static_string(value);
        }
        view
    }

    fn overlaid_call_provenance(
        &self,
        raw: &SymbolCallProvenance,
        callee: ValueId,
    ) -> SymbolCallProvenance {
        if let Some(result_identities) = self.result_identities
            && let Some(identity) = result_identities.get(&callee)
            && let Some(provenance) = identity.to_call_provenance()
        {
            return provenance;
        }
        if let Some(identity) = self.lookup_identity(raw)
            && let Some(provenance) = identity.to_call_provenance()
        {
            return provenance;
        }
        raw.clone()
    }

    fn fact_matches_clause(&self, fact: &SemanticFact, clause: &QueryClause) -> bool {
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
            callee_name.and_then(|id| self.names.resolve(id).map(Into::into));
        let call_provenance = self.overlaid_call_provenance(call_provenance, *callee);

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
                self.check_constrained_args(clause, args, unwrap.as_deref())
            }
            EventPredicate::MemberCall { member } => {
                if !member_subject_matches(
                    clause,
                    member,
                    returned_member.as_ref(),
                    instance_class.as_ref(),
                    self.names,
                ) {
                    return false;
                }
                if !member_identity_matches(
                    clause,
                    member,
                    syntactic_chain.as_ref(),
                    rooted_chain.as_ref(),
                    fact,
                    self.names,
                ) {
                    return false;
                }
                self.constraints_match(&clause.constraints, args)
            }
            _ => false,
        }
    }
}

/// Evaluate constrained clauses once over call facts.
///
/// Every candidate fact is checked by the same [`fact_matches_clause`]
/// predicate, so constraint evaluation follows one semantic path regardless
/// of how candidates were collected.
pub(in crate::analysis) fn compute_constrained_evidence_from_stream_with_overlay(
    stream: &FactStream,
    indexes: &OccurrenceIndexes,
    clauses: &[(usize, &QueryClause)],
    evidence: &mut [Vec<ClassificationEvidence>],
    overlay: Option<&ModuleOccurrenceOverlay>,
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<&BTreeMap<ValueId, LinkedModuleIdentity>>,
) {
    let Some(names) = stream.names() else {
        return;
    };
    let evaluator = MatcherEvaluator::new(names, identities, result_identities);
    let mut fallback = Vec::new();
    for (rule_index, clause) in clauses {
        let Some(candidates) = indexes.occurrences_for_clause(clause, overlay, names) else {
            fallback.push((*rule_index, clause));
            continue;
        };
        for occurrence in candidates.into_iter().filter(|occurrence| {
            stream
                .fact(occurrence.event())
                .is_some_and(|fact| evaluator.fact_matches_clause(fact, clause))
        }) {
            push_owned_evidence(
                &mut evidence[*rule_index],
                clause.evidence.kind,
                clause.evidence.symbol.clone(),
                std::iter::once(occurrence),
            );
        }
    }
    // Unsupported index shapes are intentionally handled by one shared scan,
    // rather than rescanning the fact stream once per constrained clause.
    for fact in stream.facts() {
        for (rule_index, clause) in &fallback {
            if evaluator.fact_matches_clause(fact, clause) {
                push_owned_evidence(
                    &mut evidence[*rule_index],
                    clause.evidence.kind,
                    clause.evidence.symbol.clone(),
                    std::iter::once(Occurrence::new(fact.id, fact.span)),
                );
            }
        }
    }
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
                NamePath::from_symbol_path(path, names).is_some_and(|path| chain == &path)
                    && NamePath::from_symbol_path(member, names)
                        .is_some_and(|member| chain == &member)
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
        ) => instance_class_and_chain_match(
            fact,
            syntactic_chain,
            member,
            |found_module| found_module == module,
            export,
        ),
        (
            IdentityConstraint::PackageModuleExport { module, export },
            SubjectConstraint::InstanceOf { .. },
        ) => instance_class_and_chain_match(
            fact,
            syntactic_chain,
            member,
            |found_module| module.matches(found_module),
            export,
        ),
        (IdentityConstraint::ModuleNamespace { module }, SubjectConstraint::Direct) => {
            namespace_member_matches(module_member.as_ref(), member, |found_module| {
                found_module == module
            })
        }
        (IdentityConstraint::PackageModuleNamespace { module }, SubjectConstraint::Direct) => {
            namespace_member_matches(module_member.as_ref(), member, |found_module| {
                module.matches(found_module)
            })
        }
        _ => false,
    }
}

fn instance_class_and_chain_match(
    fact: &SemanticFact,
    syntactic_chain: Option<&SymbolPath>,
    member: &SymbolPath,
    module_matches: impl FnOnce(&SmolStr) -> bool,
    export: &SmolStr,
) -> bool {
    let FactPayload::Call { instance_class, .. } = &fact.payload else {
        return false;
    };
    instance_class
        .as_ref()
        .is_some_and(|(found_module, found_export)| {
            module_matches(found_module) && found_export == export
        })
        && syntactic_chain.and_then(|s| s.last_segment()) == member.last_segment()
}

fn namespace_member_matches(
    module_member: Option<&SymbolMemberProvenance>,
    member: &SymbolPath,
    module_matches: impl FnOnce(&SmolStr) -> bool,
) -> bool {
    matches!(
        module_member,
        Some(SymbolMemberProvenance::ModuleNamespace {
            module: found_module, member: found_member
        }) if module_matches(found_module) && member.eq_chain(found_member)
    )
}

impl MatcherEvaluator<'_> {
    fn constraints_match(&self, constraints: &[QueryConstraint], args: &[CallArgInfo]) -> bool {
        constraints.iter().all(|constraint| match constraint {
            QueryConstraint::Argument(argument) => {
                args.get(argument.index()).is_some_and(|value| {
                    argument
                        .matcher()
                        .matches(&self.argument_with_overlay(value), self.names)
                })
            }
        })
    }

    fn check_constrained_args(
        &self,
        clause: &QueryClause,
        args: &[CallArgInfo],
        unwrap: Option<&CallUnwrap>,
    ) -> bool {
        unwrap.map_or_else(
            || self.constraints_match(&clause.constraints, args),
            |unwrap| self.constraints_match(&clause.constraints, &unwrap.effective_args),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Environment,
        analysis::{
            facts::{CallArgInfo, FactStream, build::build_test_stream},
            lowering::SpanNormalizer,
            matching::{LinkedModuleIdentity, ModuleExportKey, OccurrenceIndexes},
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
        Box::new([QueryConstraint::Argument(ArgumentConstraint::new(
            0,
            ValueMatcher::static_string().equals(value),
        ))])
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
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &call), (0, &member)],
            &mut evidence,
            None,
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
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &returned), (0, &instance)],
            &mut evidence,
            None,
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
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &clauses[0])],
            &mut evidence,
            None,
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
            MatcherEvaluator::new(
                &crate::analysis::name::NameTable::default(),
                Some(&identities),
                None
            )
            .argument_with_overlay(&argument)
            .static_string,
            Some("https://example.test")
        );
    }
}
