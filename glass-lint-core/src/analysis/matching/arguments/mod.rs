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
        normalized::CanonicalArgumentConstraints,
        physical::PhysicalRoot,
        rule::{EventPredicate, EvidenceDescriptor, IdentityConstraint},
    },
};

mod evaluator;
mod identity;

use evaluator::{EvaluationOperations, MatcherEvaluator, PreparedClausePaths};

/// Bundled data extracted from a `ConstrainedScan` root for the fallback
/// linear scan path.
type FallbackEntry<'a> = (
    usize,
    &'a IdentityConstraint,
    &'a EventPredicate,
    &'a CanonicalArgumentConstraints,
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
    let mut ops = EvaluationOperations::default();
    compute_constrained_inner(
        stream,
        indexes,
        roots,
        evidence,
        overlay,
        identities,
        result_identities,
        &mut ops,
    );
}

/// Inner implementation that also tracks evaluation operations.
#[allow(clippy::too_many_arguments)]
fn compute_constrained_inner(
    stream: &FactStream<Frozen>,
    indexes: &OccurrenceIndexes,
    roots: &[(usize, &PhysicalRoot)],
    evidence: &mut [Vec<ClassificationEvidence>],
    overlay: Option<&LinkedOccurrenceView<'_>>,
    identities: Option<&ModuleIdentityMap>,
    result_identities: Option<&BTreeMap<ValueId, ExportResolution>>,
    ops: &mut EvaluationOperations,
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
        &CanonicalArgumentConstraints,
        &EvidenceDescriptor,
    )> = roots
        .iter()
        .filter_map(|(rule_index, root)| match root {
            PhysicalRoot::ConstrainedScan {
                identity,
                event,
                constraints,
                evidence,
            } => Some((*rule_index, identity, event, constraints, evidence)),
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
        let mut matched: Vec<Occurrence> = Vec::new();
        for occurrence in candidates {
            if let Some(fact) = stream.fact(occurrence.event())
                && evaluator.fact_matches_clause(fact, identity, event, constraints, paths, ops)
            {
                matched.push(occurrence);
            }
        }
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
                if evaluator.fact_matches_clause(fact, identity, event, constraints, paths, ops) {
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
                physical::{PhysicalRoot, compile_argument_constraints},
                rule::{
                    CompiledMatcherPlan, EventPredicate, EvidenceDescriptor, IdentityConstraint,
                    IdentityStrength,
                },
            },
            rule::{ArgumentConstraint, ArgumentMatcher, EventQuery, ValueMatcher},
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
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().equals("/api"),
            )]),
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
        let query = EventQuery::call_heuristic("fetch")
            .unwrap()
            .with_arg_static_strings(0, ["/api"])
            .unwrap()
            .into_query();
        let plan = CompiledMatcherPlan::compile(&[query.clone(), query]).unwrap();
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
    fn missing_argument_fails_closed() {
        let stream = stream("fetch('/api');", &Environment::default());
        let _root = constrained_root(
            IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            EventPredicate::Call,
            "fetch",
        );
        // Patch the root to reference argument index 5 (out of bounds).
        let patched = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(5),
                ValueMatcher::static_string().equals("/api"),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &patched)],
            &mut evidence,
            None,
            None,
            None,
        );
        assert!(
            evidence[0].is_empty(),
            "missing argument should not produce evidence"
        );
    }

    #[test]
    fn dynamic_value_does_not_match_static_predicate() {
        let stream = stream("fetch(value);", &Environment::default());
        let root = constrained_root(
            IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            EventPredicate::Call,
            "fetch",
        );
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &root)],
            &mut evidence,
            None,
            None,
            None,
        );
        // Dynamic values should not match a static string predicate.
        assert!(
            evidence[0].is_empty(),
            "dynamic value must not match static string predicate"
        );
    }

    #[test]
    fn sparse_argument_positions() {
        let stream = stream("fetch('/api', '/path');", &Environment::default());
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(0),
                    ValueMatcher::static_string().equals("/api"),
                ),
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(1),
                    ValueMatcher::static_string().equals("/path"),
                ),
            ]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &root)],
            &mut evidence,
            None,
            None,
            None,
        );
        assert!(!evidence[0].is_empty(), "sparse arguments should match");
        assert_eq!(evidence[0][0].occurrences.len(), 1);
    }

    #[test]
    fn constraint_order_does_not_affect_matching() {
        let stream = stream("fetch('/api', '/path');", &Environment::default());
        let root_a = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(0),
                    ValueMatcher::static_string().equals("/api"),
                ),
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(1),
                    ValueMatcher::static_string().equals("/path"),
                ),
            ]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let root_b = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(1),
                    ValueMatcher::static_string().equals("/path"),
                ),
                ArgumentConstraint::new(
                    crate::api::rule::ArgumentIndex::new_unchecked(0),
                    ValueMatcher::static_string().equals("/api"),
                ),
            ]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut ev_a = vec![Vec::new()];
        let mut ev_b = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &root_a)],
            &mut ev_a,
            None,
            None,
            None,
        );
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &root_b)],
            &mut ev_b,
            None,
            None,
            None,
        );
        assert_eq!(ev_a[0].len(), ev_b[0].len());
        assert_eq!(ev_a[0][0].count, ev_b[0][0].count);
    }

    #[test]
    fn equals_any_accepts_any_matching_alternative() {
        let stream = stream("fetch('/api');", &Environment::default());
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string()
                    .equals_any(["/api", "/other"])
                    .unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &root)],
            &mut evidence,
            None,
            None,
            None,
        );
        assert!(!evidence[0].is_empty(), "equals_any should match /api");
    }

    #[test]
    fn equals_any_rejects_non_matching_values() {
        let stream = stream("fetch('/other');", &Environment::default());
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string()
                    .equals_any(["/api", "/v1"])
                    .unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &root)],
            &mut evidence,
            None,
            None,
            None,
        );
        assert!(
            evidence[0].is_empty(),
            "equals_any should reject non-matching values"
        );
    }

    #[test]
    fn contains_any_accepts_string_containing_marker() {
        let stream = stream("fetch('/api/token');", &Environment::default());
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string()
                    .contains_any(["token"])
                    .unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &root)],
            &mut evidence,
            None,
            None,
            None,
        );
        assert!(
            !evidence[0].is_empty(),
            "contains_any should match /api/token"
        );
    }

    #[test]
    fn prefix_matches_static_string_start() {
        let stream = stream(
            "fetch('https://example.test/data');",
            &Environment::default(),
        );
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string()
                    .starts_with_any(["https://"])
                    .unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &root)],
            &mut evidence,
            None,
            None,
            None,
        );
        assert!(
            !evidence[0].is_empty(),
            "prefix should match https:// string"
        );
    }

    #[test]
    fn object_keys_matcher_accepts_expected_keys() {
        let stream = stream(
            "fetch({url: '/api', method: 'POST'});",
            &Environment::default(),
        );
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ArgumentMatcher::object_keys(["url", "method"]).unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &root)],
            &mut evidence,
            None,
            None,
            None,
        );
        assert!(!evidence[0].is_empty(), "object keys should match");
    }

    #[test]
    fn object_property_value_matcher_accepts_matching_property() {
        let stream = stream(
            "fetch({url: '/api', method: 'POST'});",
            &Environment::default(),
        );
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints: compile_argument_constraints(&[ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ArgumentMatcher::object_property_value(
                    "method",
                    ValueMatcher::static_string().equals("POST"),
                )
                .unwrap(),
            )]),
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };
        let index = build_index(&stream);
        let mut evidence = vec![Vec::new()];
        compute_constrained_evidence_from_stream_with_overlay(
            &stream,
            &index,
            &[(0, &root)],
            &mut evidence,
            None,
            None,
            None,
        );
        assert!(
            !evidence[0].is_empty(),
            "object property value should match"
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

    // ── Package 6: operation and argument-preparation tests ────────

    /// Helper that runs `compute_constrained_inner` and returns ops.
    fn run_with_ops(
        stream: &FactStream<Frozen>,
        index: &OccurrenceIndexes,
        roots: &[(usize, &PhysicalRoot)],
        overlay: Option<&LinkedOccurrenceView<'_>>,
    ) -> EvaluationOperations {
        let mut evidence = vec![Vec::new(); roots.len()];
        let mut ops = EvaluationOperations::default();
        compute_constrained_inner(
            stream,
            index,
            roots,
            &mut evidence,
            overlay,
            None,
            None,
            &mut ops,
        );
        ops
    }

    #[test]
    fn two_predicates_on_one_arg_prepare_argument_once() {
        let stream = stream("fetch('/api');", &Environment::default());
        let index = build_index(&stream);

        // Two constraints on the same argument index (0):
        //   equals("/api") AND starts_with_any(["/"])
        let constraints = compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().equals("/api"),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string()
                    .starts_with_any(["/"])
                    .unwrap(),
            ),
        ]);
        assert_eq!(constraints.groups().len(), 1, "should be one group");

        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);

        // One candidate, one group → 1 argument preparation, 2 predicates
        assert_eq!(ops.candidates, 1, "one candidate (fetch call)");
        assert_eq!(ops.groups, 1, "one group");
        assert_eq!(ops.argument_preparations, 1, "argument prepared once");
        assert_eq!(ops.value_resolutions, 1, "argument value resolved once");
        assert_eq!(ops.predicates, 2, "two predicates applied");
    }

    #[test]
    fn mixed_object_predicates_share_one_prepared_projection() {
        let stream = stream(
            "fetch({url: '/api', method: 'POST'});",
            &Environment::default(),
        );
        let index = build_index(&stream);
        let constraints = compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ArgumentMatcher::object_keys(["url", "method"]).unwrap(),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ArgumentMatcher::object_property_value(
                    "method",
                    ValueMatcher::static_string().equals("POST"),
                )
                .unwrap(),
            ),
        ]);
        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);
        assert_eq!(ops.candidates, 1);
        assert_eq!(ops.groups, 1);
        assert_eq!(ops.argument_preparations, 1);
        assert_eq!(ops.value_resolutions, 1);
        assert_eq!(ops.predicates, 2);
    }

    #[test]
    fn several_argument_positions_each_prepared_once() {
        let stream = stream("fetch('/api', '/path');", &Environment::default());
        let index = build_index(&stream);

        // Constraints on index 0 AND index 1
        let constraints = compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().equals("/api"),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(1),
                ValueMatcher::static_string().equals("/path"),
            ),
        ]);
        assert_eq!(constraints.groups().len(), 2, "should be two groups");

        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);

        // One candidate, two groups → 2 argument preparations, 2 predicates
        assert_eq!(ops.candidates, 1, "one candidate");
        assert_eq!(ops.groups, 2, "two groups (index 0 and index 1)");
        assert_eq!(ops.argument_preparations, 2, "each index prepared once");
        assert_eq!(
            ops.value_resolutions, 2,
            "each argument value resolved once"
        );
        assert_eq!(ops.predicates, 2, "one predicate per group");
    }

    #[test]
    fn duplicate_constraints_do_not_inflate_operation_counts() {
        let stream = stream("fetch('/api');", &Environment::default());
        let index = build_index(&stream);

        // Four identical constraints on the same index — compile_argument_constraints
        // should deduplicate them down to one group with one predicate.
        let constraints = compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().equals("/api"),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().equals("/api"),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().equals("/api"),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().equals("/api"),
            ),
        ]);
        assert_eq!(constraints.groups().len(), 1, "deduplicated to one group");
        assert_eq!(
            constraints.groups()[0].predicates().len(),
            1,
            "deduplicated to one predicate"
        );

        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);

        // One candidate, one group, one predicate — same as the simple case.
        assert_eq!(ops.candidates, 1, "one candidate");
        assert_eq!(ops.groups, 1, "one group (deduplicated)");
        assert_eq!(ops.predicates, 1, "one predicate (deduplicated)");
        assert_eq!(
            ops.argument_preparations, 1,
            "argument prepared once despite four raw constraints"
        );
        assert_eq!(ops.value_resolutions, 1);
    }

    #[test]
    fn operation_counts_scale_with_candidates() {
        let stream = stream(
            "fetch('/a'); fetch('/b'); fetch('/c');",
            &Environment::default(),
        );
        let index = build_index(&stream);

        let constraints = compile_argument_constraints(&[ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string(),
        )]);

        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);

        assert_eq!(ops.candidates, 3, "three candidates (one per call)");
        assert_eq!(ops.groups, 3, "one group per candidate");
        assert_eq!(ops.predicates, 3, "one predicate per candidate");
    }

    #[test]
    fn static_alias_and_reassignment_preserves_matching() {
        // A static alias (`const x = '/api'; fetch(x);`) should match
        // the same as `fetch('/api');`.
        let stream = stream("const x = '/api'; fetch(x);", &Environment::default());
        let index = build_index(&stream);

        let constraints = compile_argument_constraints(&[ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string().equals("/api"),
        )]);

        let root = PhysicalRoot::ConstrainedScan {
            identity: IdentityConstraint::Any {
                name: "fetch".into(),
                strength: IdentityStrength::Heuristic,
            },
            event: EventPredicate::Call,
            constraints,
            evidence: EvidenceDescriptor {
                kind: MatchKind::CallArgument,
                symbol: "fetch".into(),
            },
        };

        let ops = run_with_ops(&stream, &index, &[(0, &root)], None);

        // The alias resolves through the value table: fetch(x) with x='/api'
        // should produce one matching candidate.
        assert_eq!(ops.candidates, 1, "one candidate (fetch(x))");
        assert_eq!(ops.groups, 1, "one group");
    }
}
