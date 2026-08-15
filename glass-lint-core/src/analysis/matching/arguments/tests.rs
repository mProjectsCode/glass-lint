use glass_lint_datastructures::PathId;

use super::*;
use crate::{
    Environment,
    analysis::{
        facts::{CallArgInfo, FactStream, Frozen, build_test_stream},
        matching::{ExportResolution, ModuleExportKey, OccurrenceIndexes},
        model::value::ValueId,
        resolution::Resolver,
        semantic::SpanNormalizer,
        syntax::SymbolCallProvenance,
    },
    api::{
        classification::MatchKind,
        compiler::{
            physical::{PhysicalRoot, compile_argument_constraints},
            rule::{CompiledMatcherPlan, EventSpec, EvidenceDescriptor, IdentityConstraint},
        },
        rule::{ArgumentConstraint, ArgumentMatcher, EventQuery, ValueMatcher},
    },
    project::SourceText,
};

fn stream(source: &str, environment: &Environment) -> FactStream<Frozen> {
    let parsed = crate::parse_test_source(source, "constrained.js").unwrap();
    let coordinates = SpanNormalizer::new(parsed.source_start, &SourceText::from(source));
    let budget = crate::analysis::SemanticBudget::default();
    let mut resolver =
        Resolver::collect_with_environment(&parsed.program, environment, coordinates, &budget);
    build_test_stream(&parsed.program, &mut resolver)
}

fn build_index(stream: &FactStream<Frozen>) -> OccurrenceIndexes {
    OccurrenceIndexes::from_stream(
        stream,
        &Environment::default(),
        crate::analysis::DerivedPhaseAvailability::Enabled,
    )
}

#[test]
fn constrained_publication_returns_capacity_error() {
    let stream = stream("fetch('/api');", &Environment::default());
    let index = build_index(&stream);
    let root = constrained_root(
        IdentityConstraint::Any {
            name: "fetch".into(),
        },
        EventSpec::Call,
        "fetch",
    );
    let mut evidence = RuleEvidenceTable::new_for_test(0);

    let error = try_compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
    )
    .expect_err("a stale rule index must remain a typed publication error");

    assert_eq!(
        error,
        RuleEvidenceError::RuleOutOfRange {
            rule: RuleIndex::new(0),
            capacity: 0,
        }
    );
}

fn constrained_root(identity: IdentityConstraint, event: EventSpec, symbol: &str) -> PhysicalRoot {
    PhysicalRoot::ConstrainedScan {
        identity,
        event,
        constraints: compile_argument_constraints(&[ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string().try_equals("/api").unwrap(),
        )]),
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: symbol.into(),
        },
    }
}

/// Helper that runs `compute_constrained_inner` and returns ops.
fn run_with_ops(
    stream: &FactStream<Frozen>,
    index: &OccurrenceIndexes,
    roots: &[ConstrainedRootInput<'_>],
    overlay: Option<&LinkedOccurrenceView<'_>>,
) -> EvaluationOperations {
    let mut evidence = RuleEvidenceTable::new_for_test(roots.len());
    let mut ops = EvaluationOperations::default();
    let artifact = MatcherArtifact::from_parts_with_overlay(stream, index, overlay);
    compute_constrained_inner(
        &artifact,
        roots,
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
        &mut ops,
    )
    .expect("test evidence uses its catalog capacity");
    ops
}

fn root_input(root: &PhysicalRoot) -> ConstrainedRootInput<'_> {
    ConstrainedRootInput::new(RuleIndex::new(0), root)
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
        },
        EventSpec::Call,
        "fetch",
    );
    let member = constrained_root(
        IdentityConstraint::Any {
            name: "client.open".into(),
        },
        EventSpec::MemberCall {
            member: "client.open".into(),
        },
        "client.open",
    );
    let index = build_index(&stream);
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&call), root_input(&member)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
    );
    assert_eq!(evidence[0].len(), 2);
    assert!(evidence[0].iter().all(|item| item.count() == 1));
    assert_ne!(
        evidence[0][0].occurrences()[0].fact(),
        evidence[0][1].occurrences()[0].fact()
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
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&roots[0])],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
    );
    assert_eq!(evidence[0].len(), 1);
    assert_eq!(evidence[0][0].occurrences().len(), 2);
    assert!(
        evidence[0][0]
            .occurrences()
            .iter()
            .all(|occ| !occ.span().is_empty())
    );
    let mut normalized = std::mem::take(&mut evidence[0]);
    crate::analysis::matching::evidence::normalize_evidence(&mut normalized, usize::MAX);
    assert_eq!(normalized.len(), 1);
    assert_eq!(normalized[0].count(), 2);
    assert_eq!(normalized[0].occurrences().len(), 2);
    assert!(
        normalized[0]
            .occurrences()
            .windows(2)
            .all(|pair| { (pair[0].span(), pair[0].fact()) < (pair[1].span(), pair[1].fact()) })
    );
}

#[test]
fn missing_argument_fails_closed() {
    let stream = stream("fetch('/api');", &Environment::default());
    let _root = constrained_root(
        IdentityConstraint::Any {
            name: "fetch".into(),
        },
        EventSpec::Call,
        "fetch",
    );
    // Patch the root to reference argument index 5 (out of bounds).
    let patched = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        constraints: compile_argument_constraints(&[ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(5),
            ValueMatcher::static_string().try_equals("/api").unwrap(),
        )]),
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };
    let index = build_index(&stream);
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&patched)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
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
        },
        EventSpec::Call,
        "fetch",
    );
    let index = build_index(&stream);
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
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
        },
        event: EventSpec::Call,
        constraints: compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(1),
                ValueMatcher::static_string().try_equals("/path").unwrap(),
            ),
        ]),
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };
    let index = build_index(&stream);
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
    );
    assert!(!evidence[0].is_empty(), "sparse arguments should match");
    assert_eq!(evidence[0][0].occurrences().len(), 1);
}

#[test]
fn constraint_order_does_not_affect_matching() {
    let stream = stream("fetch('/api', '/path');", &Environment::default());
    let root_a = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
        constraints: compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(1),
                ValueMatcher::static_string().try_equals("/path").unwrap(),
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
        },
        event: EventSpec::Call,
        constraints: compile_argument_constraints(&[
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(1),
                ValueMatcher::static_string().try_equals("/path").unwrap(),
            ),
            ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            ),
        ]),
        evidence: EvidenceDescriptor {
            kind: MatchKind::CallArgument,
            symbol: "fetch".into(),
        },
    };
    let index = build_index(&stream);
    let mut ev_a = RuleEvidenceTable::new_for_test(1);
    let mut ev_b = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root_a)],
        &mut ev_a,
        MatcherProjectOverlay::new(None, None),
    );
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root_b)],
        &mut ev_b,
        MatcherProjectOverlay::new(None, None),
    );
    assert_eq!(ev_a[0].len(), ev_b[0].len());
    assert_eq!(ev_a[0][0].count(), ev_b[0][0].count());
}

#[test]
fn equals_any_accepts_any_matching_alternative() {
    let stream = stream("fetch('/api');", &Environment::default());
    let root = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
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
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
    );
    assert!(!evidence[0].is_empty(), "equals_any should match /api");
}

#[test]
fn equals_any_rejects_non_matching_values() {
    let stream = stream("fetch('/other');", &Environment::default());
    let root = PhysicalRoot::ConstrainedScan {
        identity: IdentityConstraint::Any {
            name: "fetch".into(),
        },
        event: EventSpec::Call,
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
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
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
        },
        event: EventSpec::Call,
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
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
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
        },
        event: EventSpec::Call,
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
    let mut evidence = RuleEvidenceTable::new_for_test(1);
    compute_constrained_evidence(
        MatcherLocalInput::from_parts(&stream, &index),
        &[root_input(&root)],
        &mut evidence,
        MatcherProjectOverlay::new(None, None),
    );
    assert!(
        !evidence[0].is_empty(),
        "prefix should match https:// string"
    );
}

mod extended;
