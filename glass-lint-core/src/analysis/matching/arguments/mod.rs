use std::collections::BTreeMap;

use crate::{
    analysis::{
        facts::{FactStream, Frozen},
        matching::{
            ClassificationEvidence, LinkedOccurrenceView, ModuleIdentityMap, Occurrence,
            OccurrenceIndexes, push_owned_evidence,
        },
        project::model::ExportResolution,
        value::ValueId,
    },
    api::compiler::rule::QueryClause,
};

mod evaluator;
mod identity;

use evaluator::{MatcherEvaluator, PreparedClausePaths};

pub(in crate::analysis) fn compute_constrained_evidence_from_stream_with_overlay(
    stream: &FactStream<Frozen>,
    indexes: &OccurrenceIndexes,
    clauses: &[(usize, &QueryClause)],
    evidence: &mut [Vec<ClassificationEvidence>],
    overlay: Option<&LinkedOccurrenceView<'_>>,
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<&BTreeMap<ValueId, ExportResolution>>,
) {
    let names = stream.names();
    let values = stream.values();
    let evaluator = MatcherEvaluator::new(names, values, identities, result_identities);

    let prepared: Vec<PreparedClausePaths> = clauses
        .iter()
        .map(|(_, c)| PreparedClausePaths::new(c, names))
        .collect();

    let mut fallback: Vec<(usize, &QueryClause, &PreparedClausePaths)> = Vec::new();
    for ((rule_index, clause), paths) in clauses.iter().zip(prepared.iter()) {
        let Some(candidates) = indexes.occurrences_for_clause(clause, overlay, names) else {
            fallback.push((*rule_index, clause, paths));
            continue;
        };
        let matched: Vec<_> = candidates
            .into_iter()
            .filter(|occurrence| {
                stream
                    .fact(occurrence.event())
                    .is_some_and(|fact| evaluator.fact_matches_clause(fact, clause, paths))
            })
            .collect();
        if !matched.is_empty() {
            push_owned_evidence(
                &mut evidence[*rule_index],
                clause.evidence.kind,
                clause.evidence.symbol.clone(),
                matched,
            );
        }
    }
    let mut fallback_occurrences: Vec<Vec<Occurrence>> =
        fallback.iter().map(|_| Vec::new()).collect();
    for fact in stream.facts() {
        for (i, (_, clause, paths)) in fallback.iter().enumerate() {
            if evaluator.fact_matches_clause(fact, clause, paths) {
                fallback_occurrences[i].push(Occurrence::new(fact.id, fact.span));
            }
        }
    }
    for (i, (rule_index, clause, _paths)) in fallback.iter().enumerate() {
        let occurrences = std::mem::take(&mut fallback_occurrences[i]);
        if !occurrences.is_empty() {
            push_owned_evidence(
                &mut evidence[*rule_index],
                clause.evidence.kind,
                clause.evidence.symbol.clone(),
                occurrences,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::PathId;

    use super::*;
    use crate::{
        Environment,
        analysis::{
            facts::{CallArgInfo, FactStream, Frozen, build::build_test_stream},
            lowering::SpanNormalizer,
            matching::{ExportResolution, ModuleExportKey, OccurrenceIndexes},
            resolution::Resolver,
            syntax::SymbolCallProvenance,
            value::ValueId,
        },
        api::{
            classification::MatchKind,
            compiler::rule::{
                CompiledMatcherPlan, EventPredicate, EvidenceDescriptor, IdentityConstraint,
                IdentityStrength, QueryClause, QueryConstraint, SubjectConstraint,
            },
            rule::{ArgumentConstraint, MatcherDecl, ValueMatcher},
        },
    };

    fn stream(source: &str, environment: &Environment) -> FactStream<Frozen> {
        let parsed = crate::parse(source, "constrained.js").unwrap();
        let coordinates = SpanNormalizer::new(parsed.source_start, source);
        let mut resolver =
            Resolver::collect_with_environment(&parsed.program, environment, coordinates);
        build_test_stream(&parsed.program, &mut resolver)
    }

    fn build_index(stream: &FactStream<Frozen>) -> OccurrenceIndexes {
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
        let declaration = MatcherDecl::builder()
            .call_heuristic("fetch")
            .arg_static_strings(0, ["/api"])
            .build()
            .unwrap();
        let plan = CompiledMatcherPlan::compile_decls(&[declaration.clone(), declaration]).unwrap();
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
        assert_eq!(evidence[0].len(), 1);
        assert_eq!(evidence[0][0].occurrences.len(), 2);
        assert!(
            evidence[0][0]
                .occurrences
                .iter()
                .all(|occ| !occ.span.is_empty())
        );
        let mut normalized = std::mem::take(&mut evidence[0]);
        crate::analysis::matching::evidence::normalize_evidence(&mut normalized, usize::MAX);
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
            ExportResolution::StaticString {
                value: "https://example.test".into(),
            },
        );
        let argument = CallArgInfo {
            value: ValueId(7),
            base_value: ValueId::UNKNOWN,
            base_path: PathId::EMPTY,
            spread: false,
            provenance: SymbolCallProvenance::ModuleExport {
                module: "api".into(),
                export: "request".into(),
            },
        };
        assert_eq!(
            MatcherEvaluator::new(
                &glass_lint_datastructures::NameTable::default(),
                &crate::analysis::value::ValueTable::default(),
                Some(&identities),
                None
            )
            .argument_with_overlay(&argument)
            .static_string,
            Some("https://example.test")
        );
    }
}
