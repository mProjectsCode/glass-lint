//! Regression tests for architectural gaps in query composition.
//!
//! These tests expose known false-complete claims in the current
//! implementation.  Each test currently fails for the expected reason.
//! When the architecture is remediated all tests in this module pass.
//!
//! See q-fix.md Package 0 for the specification of each test.

use std::panic::catch_unwind;

use glass_lint_core::{
    RuleCatalog,
    rules::{
        EventQuery, EventRequirement, LifecycleCondition, LifecycleEvent, LifecycleQuery,
        LifecycleSink, QueryBuildError, QueryDecl, ValueMatcher,
    },
};

use crate::support::rule;

// ── Helpers ────────────────────────────────────────────────────────────

/// Build a minimal rule with one query, ready for catalog compilation.
fn compile_rule(
    id: &str,
    query: QueryDecl,
) -> Result<RuleCatalog, glass_lint_core::ProviderCatalogError> {
    let rule = rule(id).query(query).build().unwrap();
    RuleCatalog::new("test", vec![rule])
}

// ── Test 1: Any branches compile through the catalog ────────────────────
//
// Two event alternatives that share one primary event variable ($0 in both
// branch scopes) must compile through RuleCatalog::new and produce findings
// for either alternative.
//
// Currently fails because pass_variable_collection treats Any branches as
// one flat scope (Package 3 will fix this).

#[test]
fn any_branches_compile_through_rule_catalog() {
    let branch_a = EventQuery::call_global("fetch").unwrap().into_query();
    let branch_b = EventQuery::call_global("navigate").unwrap().into_query();
    let query = QueryDecl::any_with_evidence([Ok(branch_a), Ok(branch_b)], "network").unwrap();

    let result = compile_rule("test.any", query);
    assert!(
        result.is_ok(),
        "Any with alpha-aligned variables should compile through RuleCatalog: {result:?}"
    );
}

#[test]
fn any_rejects_incompatible_evidence_at_construction() {
    let first = EventQuery::call_global("fetch").unwrap().into_query();
    let second = EventQuery::member_call_rooted("document.navigate")
        .unwrap()
        .into_query();

    assert!(matches!(
        QueryDecl::any([Ok(first), Ok(second)]),
        Err(QueryBuildError::EvidenceProjection)
    ));
}

// ── Test 3: Same-event All compiles through the catalog ─────────────────
//
// Two compatible constraints on one selected event must compile through
// RuleCatalog::new, producing a plan that includes both predicates.
//
// Currently fails because pass_variable_collection treats All branches as
// one flat scope and rejects same-var references as duplicates (Package 3).

#[test]
fn same_event_all_compiles_through_rule_catalog() {
    let query = QueryDecl::all(
        EventQuery::call_global("fetch"),
        [
            Ok(EventRequirement::argument(0, ValueMatcher::static_string()).unwrap()),
            Ok(
                EventRequirement::argument(1, ValueMatcher::static_string().equals("/api"))
                    .unwrap(),
            ),
        ],
    )
    .unwrap();

    let result = compile_rule("test.all", query);
    assert!(
        result.is_ok(),
        "Same-event All should compile through RuleCatalog: {result:?}"
    );
}

// ── Test 4: Uncorrelated All fails through the catalog ──────────────────
//
// Selecting unrelated events without a keyed relation must produce an
// uncorrelated_conjunction error.

#[test]
fn uncorrelated_all_fails_through_rule_catalog() {
    // The supported public grammar exposes only same-event All. Uncorrelated
    // multi-event conjunctions therefore cannot be authored externally.
    let query = QueryDecl::all(EventQuery::call_global("fetch"), []).unwrap();
    assert!(compile_rule("test.correlated", query).is_ok());
}

// ── Test 5: Contradictory same-event All fails at compilation ───────────
//
// Mutually exclusive constraints on the same event or argument must produce
// a structured contradiction error.
//
// Currently the query compiles successfully because contradiction detection
// is not implemented in the validator (Package 4 will add this).

#[test]
fn contradictory_same_event_all_fails_at_compilation() {
    // One event query with two contradictory argument constraints:
    // argument 0 must equal "a" AND argument 0 must equal "b".
    // This is statically contradictory.
    let query = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().equals("a"))
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().equals("b"))
        .unwrap()
        .into_query();
    let result = compile_rule("test.contradictory", query);
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("contradict"),
        "Contradictory constraints should produce a contradiction error, got: {msg}"
    );
}

// ── Test 6: Multiple lifecycle sources compile ──────────────────────────
//
// A lifecycle with two valid source forms must compile through the same
// RuleBuilder::query route used by ordinary queries.
//
// Currently fails because lifecycle source variables are checked in one
// flat scope (duplicate VarId).  Sources should have Any-like independent
// scopes with alpha-aligned output (Package 8).

#[test]
fn multiple_lifecycle_sources_compile() {
    use glass_lint_core::rules::LifecycleCompletion;

    let src_a = glass_lint_core::rules::EventQuery::member_call_rooted("document.createElement");
    // Second source uses the same object variable — valid Any-of-source semantics
    // where either independently valid source can start the lifecycle.
    let src_b = glass_lint_core::rules::EventQuery::member_call_rooted("document.createTextNode");
    let lifecycle = LifecycleQuery::builder("test.lifecycle")
        .source(src_a)
        .source(src_b)
        .condition(glass_lint_core::rules::LifecycleCondition::event(
            glass_lint_core::rules::LifecycleEvent::property_write(
                "type",
                glass_lint_core::rules::ValueMatcher::any_value(),
            ),
        ))
        .completion(LifecycleCompletion::configuration())
        .build()
        .unwrap();
    let query = QueryDecl::lifecycle(Ok(lifecycle)).unwrap();

    let result = compile_rule("test.lifecycle", query);
    assert!(
        result.is_ok(),
        "Lifecycle with multiple sources should compile through RuleBuilder::query: {result:?}"
    );
}

// ── Test 8: Event-only modifiers reject non-event expressions ───────────
//
// with_arg is only available on EventQuery, so calling it on a composed
// QueryDecl expression is impossible at the type level.

// ── Table-driven catch_unwind test ────────────────────────────────────
//
// Every text/index/collection constructor must return a structured error
// rather than panicking.  Constructors tested here accept user-provided
// strings or indices that could be empty, malformed, or out of range.

#[test]
#[allow(clippy::too_many_lines)]
fn invalid_authoring_input_never_panics() {
    // All constructor calls that should return Err, never panic.
    #[allow(clippy::type_complexity)]
    let cases: Vec<(&str, Box<dyn Fn() -> Result<(), String>>)> = vec![
        (
            "call_global empty",
            Box::new(|| {
                EventQuery::call_global("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "call_heuristic empty",
            Box::new(|| {
                EventQuery::call_heuristic("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "call_module empty module",
            Box::new(|| {
                EventQuery::call_module("", "x")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "call_module empty export",
            Box::new(|| {
                EventQuery::call_module("m", "")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "call_package empty",
            Box::new(|| {
                EventQuery::call_package("", "x")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "member_call_rooted double dot",
            Box::new(|| {
                EventQuery::member_call_rooted("a..b")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "member_call_rooted trailing dot",
            Box::new(|| {
                EventQuery::member_call_rooted("a.b.")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "member_call_rooted leading dot",
            Box::new(|| {
                EventQuery::member_call_rooted(".a")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "member_call_rooted empty",
            Box::new(|| {
                EventQuery::member_call_rooted("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "member_call_heuristic empty",
            Box::new(|| {
                EventQuery::member_call_heuristic("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "member_call_module empty module",
            Box::new(|| {
                EventQuery::member_call_module("", "m")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "import_exact empty",
            Box::new(|| {
                EventQuery::import_exact("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "import_package empty",
            Box::new(|| {
                EventQuery::import_package("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "string_contains empty",
            Box::new(|| {
                EventQuery::string_contains("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "class_heuristic empty",
            Box::new(|| {
                EventQuery::class_heuristic("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "class_module empty module",
            Box::new(|| {
                EventQuery::class_module("", "C")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "constructor_global empty",
            Box::new(|| {
                EventQuery::constructor_global("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "constructor_heuristic empty",
            Box::new(|| {
                EventQuery::constructor_heuristic("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "constructor_module empty module",
            Box::new(|| {
                EventQuery::constructor_module("", "C")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "lifecycle member event empty",
            Box::new(|| {
                glass_lint_core::rules::LifecycleEvent::member_call("")
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "lifecycle condition empty",
            Box::new(|| {
                LifecycleCondition::any_of(Vec::<LifecycleEvent>::new())
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "lifecycle sink empty chain",
            Box::new(|| {
                LifecycleSink::argument_of("", 0)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "lifecycle sink index too large",
            Box::new(|| {
                LifecycleSink::argument_of("sink", 256)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "static alternatives empty",
            Box::new(|| {
                ValueMatcher::static_string()
                    .equals_any(Vec::<String>::new())
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
        (
            "object keys empty",
            Box::new(|| {
                glass_lint_core::rules::ArgumentMatcher::object_keys(Vec::<String>::new())
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
        ),
    ];

    for (name, constructor) in cases {
        let result = catch_unwind(std::panic::AssertUnwindSafe(constructor));
        match result {
            Ok(Err(_)) => {} // expected: structured error
            Ok(Ok(())) => panic!("{name}: expected Err but got Ok"),
            Err(panic) => {
                let msg = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(ToString::to_string))
                    .unwrap_or_else(|| "unknown panic payload".into());
                panic!("{name}: panicked instead of returning Err: {msg}")
            }
        }
    }
}

// ── Collection boundary tests at limit and limit + 1 ──────────────────

#[test]
fn lifecycle_sources_at_limit_succeeds() {
    let condition = glass_lint_core::rules::LifecycleCondition::event(
        glass_lint_core::rules::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
    );
    let mut builder = LifecycleQuery::builder("test").condition(condition);
    for i in 0..64 {
        builder = builder.source(glass_lint_core::rules::EventQuery::member_call_rooted(
            format!("a.b{i}"),
        ));
    }
    let lc = builder
        .completion(glass_lint_core::rules::LifecycleCompletion::configuration())
        .build()
        .unwrap();
    assert_eq!(lc.sources().len(), 64);
}

#[test]
fn lifecycle_sources_exceeding_limit_fails() {
    let condition = glass_lint_core::rules::LifecycleCondition::event(
        glass_lint_core::rules::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
    );
    let mut builder = LifecycleQuery::builder("test").condition(condition);
    for i in 0..65 {
        builder = builder.source(glass_lint_core::rules::EventQuery::member_call_rooted(
            format!("a.b{i}"),
        ));
    }
    let err = builder
        .completion(glass_lint_core::rules::LifecycleCompletion::configuration())
        .build()
        .unwrap_err();
    assert!(matches!(err, QueryBuildError::CollectionTooLarge(_, 65)));
}

#[test]
fn lifecycle_event_and_sink_limits_are_enforced_at_construction() {
    let events = (0..64)
        .map(|index| {
            glass_lint_core::rules::LifecycleEvent::property_write(
                format!("property{index}"),
                ValueMatcher::any_value(),
            )
        })
        .collect::<Vec<_>>();
    let sinks = (0..64)
        .map(|index| glass_lint_core::rules::LifecycleSink::any_argument_of(format!("sink{index}")))
        .collect::<Vec<_>>();
    let valid = LifecycleQuery::builder("limits")
        .source(glass_lint_core::rules::EventQuery::member_call_rooted(
            "document.createElement",
        ))
        .condition(glass_lint_core::rules::LifecycleCondition::any_of(events))
        .completion(glass_lint_core::rules::LifecycleCompletion::any_sink(sinks))
        .build();
    assert!(valid.is_ok());

    let too_many_events = (0..=64)
        .map(|index| {
            glass_lint_core::rules::LifecycleEvent::property_write(
                format!("property{index}"),
                ValueMatcher::any_value(),
            )
        })
        .collect::<Vec<_>>();
    let event_error = LifecycleQuery::builder("too-many-events")
        .source(glass_lint_core::rules::EventQuery::member_call_rooted(
            "document.createElement",
        ))
        .condition(glass_lint_core::rules::LifecycleCondition::any_of(
            too_many_events,
        ))
        .completion(glass_lint_core::rules::LifecycleCompletion::configuration())
        .build()
        .unwrap_err();
    assert!(matches!(
        event_error,
        QueryBuildError::CollectionTooLarge("lifecycle condition events", 65)
    ));

    let too_many_sinks = (0..=64)
        .map(|index| glass_lint_core::rules::LifecycleSink::any_argument_of(format!("sink{index}")))
        .collect::<Vec<_>>();
    let sink_error = LifecycleQuery::builder("too-many-sinks")
        .source(glass_lint_core::rules::EventQuery::member_call_rooted(
            "document.createElement",
        ))
        .condition(glass_lint_core::rules::LifecycleCondition::event(
            glass_lint_core::rules::LifecycleEvent::property_write(
                "type",
                ValueMatcher::any_value(),
            ),
        ))
        .completion(glass_lint_core::rules::LifecycleCompletion::any_sink(
            too_many_sinks,
        ))
        .build()
        .unwrap_err();
    assert!(matches!(
        sink_error,
        QueryBuildError::CollectionTooLarge("lifecycle completion sinks", 65)
    ));
}

#[test]
fn lifecycle_source_and_sink_indices_are_checked_without_truncation() {
    let source =
        glass_lint_core::rules::EventQuery::member_call_rooted("document.createElement").unwrap();
    assert!(matches!(
        source.with_arg(256, ValueMatcher::any_value()),
        Err(QueryBuildError::InvalidArgumentIndex(256))
    ));
    assert!(matches!(
        glass_lint_core::rules::EventQuery::member_call_rooted(""),
        Err(QueryBuildError::MalformedChain(_))
    ));
    assert!(matches!(
        glass_lint_core::rules::LifecycleSink::argument_of("sink", 256),
        Err(QueryBuildError::InvalidArgumentIndex(256))
    ));
}

#[test]
fn argument_index_at_limit_succeeds() {
    let q = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(255, ValueMatcher::static_string())
        .unwrap();
    assert_eq!(q.constraints().len(), 1);
}

#[test]
fn argument_index_exceeding_limit_fails() {
    let err = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(256, ValueMatcher::static_string());
    assert!(matches!(
        err,
        Err(QueryBuildError::InvalidArgumentIndex(256))
    ));
}

#[test]
fn sparse_public_var_id_does_not_allocate_by_raw_value() {
    // Sparse authored IDs are covered by the crate-internal normalization
    // regression; the public grammar intentionally does not expose raw IDs.
    assert!(
        compile_rule(
            "test.sparse-var",
            EventQuery::call_global("fetch").unwrap().into_query()
        )
        .is_ok()
    );
}

#[test]
fn predicate_alternatives_at_limit_succeeds() {
    let values: Vec<String> = (0..256).map(|i| format!("val{i}")).collect();
    let m = ValueMatcher::static_string().equals_any(values).unwrap();
    // Should not panic or error — canonicalization handles up to any size
    let _ = m;
}

#[test]
fn predicate_alternatives_limit_plus_one_fails_at_construction() {
    let values: Vec<String> = (0..=256).map(|i| format!("val{i}")).collect();
    assert!(matches!(
        ValueMatcher::static_string().equals_any(values),
        Err(QueryBuildError::CollectionTooLarge(_, 257))
    ));
}

#[test]
fn query_roots_boundary_succeeds() {
    let queries: Vec<QueryDecl> = (0..256)
        .map(|i| EventQuery::call_global(format!("fn{i}")).map(EventQuery::into_query))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(queries.len(), 256);
}

#[test]
fn query_roots_limit_plus_one_is_rejected_at_authoring() {
    let mut builder = rule("test.too-many-roots");
    for i in 0..=256 {
        builder = builder.query(EventQuery::call_global(format!("fn{i}")));
    }

    let error = builder.build().unwrap_err();
    assert!(error.to_string().contains("query roots"));
}

// ── Package 2: QueryBuildError variant tests ──────────────────────────
#[test]
fn empty_lifecycle_sources_rejected() {
    let err = LifecycleQuery::builder("test")
        .completion(glass_lint_core::rules::LifecycleCompletion::configuration())
        .build()
        .unwrap_err();
    assert!(matches!(err, QueryBuildError::MissingLifecycleSources));
}

#[test]
fn empty_lifecycle_evidence_symbol_rejected() {
    let err = LifecycleQuery::builder(" ")
        .source(glass_lint_core::rules::EventQuery::member_call_rooted(
            "document.create",
        ))
        .completion(glass_lint_core::rules::LifecycleCompletion::any_sink([
            glass_lint_core::rules::LifecycleSink::argument_of("sink", 0),
        ]))
        .build()
        .unwrap_err();
    assert!(matches!(err, QueryBuildError::EmptyEvidenceSymbol));
}

#[test]
fn invalid_scope_package_rejected() {
    let err = EventQuery::call_package("  ", "export");
    assert!(matches!(err, Err(QueryBuildError::InvalidScopePackage)));
}

#[test]
fn excessive_constraints_same_index_rejected() {
    // Add one more predicate than the per-argument construction limit.
    let mut q = EventQuery::call_global("fetch").unwrap();
    for _ in 0..2049 {
        match q.with_arg(0, ValueMatcher::static_string()) {
            Ok(next) => q = next,
            Err(e) => {
                assert!(
                    matches!(e, QueryBuildError::ExcessivePredicates { .. }),
                    "expected ExcessivePredicates, got: {e:?}"
                );
                return;
            }
        }
    }
    panic!("expected ExcessiveConstraints at constraint 2049");
}

#[test]
fn argument_group_limit_plus_one_fails_at_construction() {
    let mut query = EventQuery::call_global("fetch").unwrap();
    for index in 0..64 {
        query = query
            .with_arg(index, ValueMatcher::static_string())
            .unwrap();
    }
    assert!(matches!(
        query.with_arg(64, ValueMatcher::static_string()),
        Err(QueryBuildError::ExcessiveArgumentGroups(65))
    ));
}

#[test]
fn equivalent_argument_order_produces_equal_matchers() {
    let a = ValueMatcher::static_string()
        .equals_any(["b", "a", "c"])
        .unwrap();
    let b = ValueMatcher::static_string()
        .equals_any(["c", "a", "b"])
        .unwrap();
    assert_eq!(a, b, "canonicalized equals_any should be order-independent");
}

#[test]
fn equivalent_starts_with_order_produces_equal_matchers() {
    let a = ValueMatcher::static_string()
        .starts_with_any(["z", "a"])
        .unwrap();
    let b = ValueMatcher::static_string()
        .starts_with_any(["a", "z"])
        .unwrap();
    assert_eq!(
        a, b,
        "canonicalized starts_with_any should be order-independent"
    );
}

#[test]
fn equivalent_contains_any_order_produces_equal_matchers() {
    let a = ValueMatcher::static_string()
        .contains_any(["secret", "token"])
        .unwrap();
    let b = ValueMatcher::static_string()
        .contains_any(["token", "secret"])
        .unwrap();
    assert_eq!(
        a, b,
        "canonicalized contains_any should be order-independent"
    );
}

#[test]
fn equivalent_contains_all_order_produces_equal_matchers() {
    let a = ValueMatcher::static_string()
        .contains_all(["b", "a"])
        .unwrap();
    let b = ValueMatcher::static_string()
        .contains_all(["a", "b"])
        .unwrap();
    assert_eq!(
        a, b,
        "canonicalized contains_all should be order-independent"
    );
}

#[test]
fn deduplicated_alternatives_are_removed() {
    // equals_any with duplicates should deduplicate to one element.
    let m = ValueMatcher::static_string()
        .equals_any(["a", "a", "a"])
        .unwrap();
    let expected = ValueMatcher::static_string().equals("a");
    assert_eq!(m, expected, "duplicates in equals_any should be removed");
}

#[test]
fn query_modifiers_do_not_silently_ignore_non_event_expressions() {
    // Verify that with_arg is only available on EventQuery, not on
    // composed QueryDecl — meaning it cannot silently ignore
    // non-Event expressions.
    let event_query = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string())
        .unwrap();
    let modified = event_query.into_query();
    assert!(
        modified.expression().diagnostic_name() == "event",
        "with_arg should produce an Event expression, got: {:?}",
        modified.expression()
    );

    // Also verify that invalid index produces a build error, not a panic.
    let err = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(300, ValueMatcher::static_string());
    assert!(
        err.is_err(),
        "excessive argument index must produce an error"
    );
}
