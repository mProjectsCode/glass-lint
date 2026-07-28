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
    api::compiler::{
        physical::PhysicalRoot,
        rule::{EventPredicate, EvidenceDescriptor, IdentityConstraint, QueryConstraint},
    },
};

mod evaluator;
mod identity;

use evaluator::{MatcherEvaluator, PreparedClausePaths};

/// Bundled data extracted from a `ConstrainedScan` root for the fallback
/// linear scan path.
type FallbackEntry<'a> = (
    usize,
    &'a IdentityConstraint,
    &'a EventPredicate,
    &'a [QueryConstraint],
    &'a EvidenceDescriptor,
    &'a PreparedClausePaths,
);

pub(in crate::analysis) fn compute_constrained_evidence_from_stream_with_overlay(
    stream: &FactStream<Frozen>,
    indexes: &OccurrenceIndexes,
    roots: &[(usize, &PhysicalRoot)],
    evidence: &mut [Vec<ClassificationEvidence>],
    overlay: Option<&LinkedOccurrenceView<'_>>,
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<&BTreeMap<ValueId, ExportResolution>>,
) {
    let names = stream.names();
    let values = stream.values();
    let evaluator = MatcherEvaluator::new(names, values, identities, result_identities);

    // Extract only ConstrainedScan roots (the constrained path only handles
    // these; other root types are handled by the physical plan executor).
    let constrained: Vec<(
        usize,
        &IdentityConstraint,
        &EventPredicate,
        &[QueryConstraint],
        &EvidenceDescriptor,
    )> = roots
        .iter()
        .filter_map(|(rule_index, root)| match root {
            PhysicalRoot::ConstrainedScan {
                identity,
                event,
                constraints,
                evidence,
            } => Some((*rule_index, identity, event, constraints.as_ref(), evidence)),
            _ => None,
        })
        .collect();

    let prepared: Vec<PreparedClausePaths> = constrained
        .iter()
        .map(|(_, identity, event, _, _)| PreparedClausePaths::new(identity, event, names))
        .collect();

    // Phase 1: Index-based candidate lookup.
    // When the index lookup succeeds, candidates are filtered through the
    // evaluator.  Roots whose index lookup fails are collected for the
    // fallback linear scan (Phase 2).
    let mut fallback: Vec<FallbackEntry<'_>> = Vec::new();
    for ((rule_index, identity, event, constraints, evidence_desc), paths) in
        constrained.iter().zip(prepared.iter())
    {
        let Some(candidates) = indexes.occurrences_for_indexed(identity, event, overlay, names)
        else {
            fallback.push((
                *rule_index,
                identity,
                event,
                constraints,
                evidence_desc,
                paths,
            ));
            continue;
        };
        let matched: Vec<_> = candidates
            .into_iter()
            .filter(|occurrence| {
                stream.fact(occurrence.event()).is_some_and(|fact| {
                    evaluator.fact_matches_clause(fact, identity, event, constraints, paths)
                })
            })
            .collect();
        if !matched.is_empty() {
            push_owned_evidence(
                &mut evidence[*rule_index],
                evidence_desc.kind,
                evidence_desc.symbol.clone(),
                matched,
            );
        }
    }

    // Phase 2: Fallback linear scan for roots that the index could not
    // resolve.  This handles cases where the call provenance is resolved
    // through overlays (e.g., returned callables) rather than direct
    // module/global index entries.
    if !fallback.is_empty() {
        let mut fallback_occurrences: Vec<Vec<Occurrence>> =
            fallback.iter().map(|_| Vec::new()).collect();
        for fact in stream.facts() {
            for (i, (_, identity, event, constraints, _, paths)) in fallback.iter().enumerate() {
                if evaluator.fact_matches_clause(fact, identity, event, constraints, paths) {
                    fallback_occurrences[i].push(Occurrence::new(fact.id, fact.span));
                }
            }
        }
        for (i, (rule_index, _, _, _, evidence_desc, _)) in fallback.iter().enumerate() {
            let occurrences = std::mem::take(&mut fallback_occurrences[i]);
            if !occurrences.is_empty() {
                push_owned_evidence(
                    &mut evidence[*rule_index],
                    evidence_desc.kind,
                    evidence_desc.symbol.clone(),
                    occurrences,
                );
            }
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
            facts::{CallArgInfo, FactStream, Frozen, build_test_stream},
            lowering::SpanNormalizer,
            matching::{ExportResolution, ModuleExportKey, OccurrenceIndexes},
            resolution::Resolver,
            syntax::SymbolCallProvenance,
            value::ValueId,
        },
        api::{
            classification::MatchKind,
            compiler::{
                physical::PhysicalRoot,
                rule::{
                    CompiledMatcherPlan, EventPredicate, EvidenceDescriptor, IdentityConstraint,
                    IdentityStrength, QueryConstraint,
                },
            },
            rule::{ArgumentConstraint, MatcherDecl, ValueMatcher},
        },
        project::SourceText,
    };

    fn stream(source: &str, environment: &Environment) -> FactStream<Frozen> {
        let parsed = crate::parse(source, "constrained.js").unwrap();
        let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(source));
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

    fn constrained_root(
        identity: IdentityConstraint,
        event: EventPredicate,
        symbol: &str,
    ) -> PhysicalRoot {
        PhysicalRoot::ConstrainedScan {
            identity,
            event,
            constraints: Box::new([QueryConstraint::Argument(ArgumentConstraint::new(
                0,
                ValueMatcher::static_string().equals("/api"),
            ))]),
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
        let call = constrained_root(
            IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            EventPredicate::Call,
            "fetch",
        );
        let member = constrained_root(
            IdentityConstraint::Any {
                name: "client.open".into(),
                strength: IdentityStrength::Heuristic,
            },
            EventPredicate::MemberCall {
                member: "client.open".into(),
            },
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
    fn constrained_evidence_is_source_ordered_and_deduplicated() {
        let declaration = MatcherDecl::builder()
            .call_heuristic("fetch")
            .arg_static_strings(0, ["/api"])
            .build()
            .unwrap();
        let plan = CompiledMatcherPlan::compile_decls(&[declaration.clone(), declaration]).unwrap();
        let roots: Vec<PhysicalRoot> = plan
            .physical_roots()
            .iter()
            .filter(|r| matches!(r, PhysicalRoot::ConstrainedScan { .. }))
            .cloned()
            .collect();
        assert_eq!(roots.len(), 1, "equivalent declarations produce one root");

        let stream = stream("fetch('/api');\nfetch('/api');", &Environment::default());
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &roots[0])],
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
