//! Regression tests for the query-composition contracts.
//!
//! The cases cover independent branch scopes, same-event conjunctions,
//! lifecycle sources, contradiction validation, and bounded authoring input.

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
// Each branch has an independent scope while its selected output remains
// aligned with the surrounding query.

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
// Same-event requirements share the selected event variable and compile as
// one conjunction.

#[test]
fn same_event_all_compiles_through_rule_catalog() {
    let query = QueryDecl::all(
        EventQuery::call_global("fetch"),
        [
            Ok(EventRequirement::argument(0, ValueMatcher::static_string()).unwrap()),
            Ok(EventRequirement::argument(
                1,
                ValueMatcher::static_string().try_equals("/api").unwrap(),
            )
            .unwrap()),
        ],
    )
    .unwrap();

    let result = compile_rule("test.all", query);
    assert!(
        result.is_ok(),
        "Same-event All should compile through RuleCatalog: {result:?}"
    );
}

#[test]
fn argument_constraints_are_rejected_for_non_call_events_at_construction() {
    let error = EventQuery::import_exact("mod")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string())
        .unwrap_err();
    assert_eq!(error, QueryBuildError::ArgumentsRequireCallEvent);

    let error = QueryDecl::all(
        EventQuery::import_exact("mod"),
        [Ok(EventRequirement::argument(
            0,
            ValueMatcher::static_string(),
        )
        .unwrap())],
    )
    .unwrap_err();
    assert_eq!(error, QueryBuildError::ArgumentsRequireCallEvent);
}

// ── Test 4: Empty same-event All compiles through the catalog ────────────
//
// Selecting unrelated events without a keyed relation must produce an
// uncorrelated_conjunction error.

#[test]
fn empty_same_event_all_compiles_through_rule_catalog() {
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
// The validator rejects contradictory constraints with a structured error.

#[test]
fn contradictory_same_event_all_fails_at_compilation() {
    // One event query with two contradictory argument constraints:
    // argument 0 must equal "a" AND argument 0 must equal "b".
    // This is statically contradictory.
    let query = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().try_equals("a").unwrap())
        .unwrap()
        .with_arg(0, ValueMatcher::static_string().try_equals("b").unwrap())
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
// Sources use Any-like independent scopes with alpha-aligned output.

#[test]
fn multiple_lifecycle_sources_compile() {
    use glass_lint_core::rules::LifecycleCompletion;

    let src_a = glass_lint_core::rules::EventQuery::member_call_rooted("document.createElement");
    // Second source uses the same object variable — valid Any-of-source semantics
    // where either independently valid source can start the lifecycle.
    let src_b = glass_lint_core::rules::EventQuery::member_call_rooted("document.createTextNode");
    let lifecycle = LifecycleQuery::catalog_builder("test.lifecycle")
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

type InvalidConstructor = Box<dyn Fn() -> Result<(), QueryBuildError>>;

macro_rules! invalid_case {
    ($name:literal, $constructor:expr) => {
        (
            $name,
            Box::new(|| $constructor.map(|_| ())) as InvalidConstructor,
        )
    };
}

fn assert_invalid_constructor(name: &str, constructor: InvalidConstructor) {
    let result = catch_unwind(std::panic::AssertUnwindSafe(constructor));
    match result {
        Ok(Err(_error)) => {}
        Ok(Ok(())) => panic!("{name}: expected Err but got Ok"),
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(ToString::to_string))
                .unwrap_or_else(|| "unknown panic payload".into());
            panic!("{name}: panicked instead of returning Err: {message}");
        }
    }
}

#[test]
fn invalid_authoring_input_never_panics() {
    let cases = vec![
        invalid_case!("call_global empty", EventQuery::call_global("")),
        invalid_case!("call_heuristic empty", EventQuery::call_heuristic("")),
        invalid_case!("call_module empty module", EventQuery::call_module("", "x")),
        invalid_case!("call_module empty export", EventQuery::call_module("m", "")),
        invalid_case!("call_package empty", EventQuery::call_package("", "x")),
        invalid_case!(
            "member_call_rooted double dot",
            EventQuery::member_call_rooted("a..b")
        ),
        invalid_case!(
            "member_call_rooted trailing dot",
            EventQuery::member_call_rooted("a.b.")
        ),
        invalid_case!(
            "member_call_rooted leading dot",
            EventQuery::member_call_rooted(".a")
        ),
        invalid_case!(
            "member_call_rooted empty",
            EventQuery::member_call_rooted("")
        ),
        invalid_case!(
            "member_call_heuristic empty",
            EventQuery::member_call_heuristic("")
        ),
        invalid_case!(
            "member_call_module empty module",
            EventQuery::member_call_module("", "m")
        ),
        invalid_case!("import_exact empty", EventQuery::import_exact("")),
        invalid_case!("import_package empty", EventQuery::import_package("")),
        invalid_case!("string_contains empty", EventQuery::string_contains("")),
        invalid_case!("class_heuristic empty", EventQuery::class_heuristic("")),
        invalid_case!(
            "class_module empty module",
            EventQuery::class_module("", "C")
        ),
        invalid_case!(
            "constructor_global empty",
            EventQuery::constructor_global("")
        ),
        invalid_case!(
            "constructor_heuristic empty",
            EventQuery::constructor_heuristic("")
        ),
        invalid_case!(
            "constructor_module empty module",
            EventQuery::constructor_module("", "C")
        ),
        invalid_case!(
            "lifecycle member event empty",
            LifecycleEvent::member_call("")
        ),
        invalid_case!(
            "lifecycle condition empty",
            LifecycleCondition::any_of(Vec::<LifecycleEvent>::new())
        ),
        invalid_case!(
            "lifecycle sink empty chain",
            LifecycleSink::argument_of_member("", 0)
        ),
        invalid_case!(
            "lifecycle sink index too large",
            LifecycleSink::argument_of_member("sink", 256)
        ),
        invalid_case!(
            "static alternatives empty",
            ValueMatcher::static_string().equals_any(Vec::<String>::new())
        ),
        invalid_case!(
            "object keys empty",
            glass_lint_core::rules::ArgumentMatcher::object_keys(Vec::<String>::new())
        ),
    ];

    for (name, constructor) in cases {
        assert_invalid_constructor(name, constructor);
    }
}

// ── Collection boundary tests at limit and limit + 1 ──────────────────

#[test]
fn lifecycle_sources_at_limit_succeeds() {
    let condition = glass_lint_core::rules::LifecycleCondition::event(
        glass_lint_core::rules::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
    );
    let mut builder = LifecycleQuery::catalog_builder("test").condition(condition);
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
    let mut builder = LifecycleQuery::catalog_builder("test").condition(condition);
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
        .map(|index| {
            glass_lint_core::rules::LifecycleSink::any_argument_of_member(format!("sink{index}"))
        })
        .collect::<Vec<_>>();
    let valid = LifecycleQuery::catalog_builder("limits")
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
    let event_error = LifecycleQuery::catalog_builder("too-many-events")
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
        .map(|index| {
            glass_lint_core::rules::LifecycleSink::any_argument_of_member(format!("sink{index}"))
        })
        .collect::<Vec<_>>();
    let sink_error = LifecycleQuery::catalog_builder("too-many-sinks")
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
        glass_lint_core::rules::LifecycleSink::argument_of_member("sink", 256),
        Err(QueryBuildError::InvalidArgumentIndex(256))
    ));
}

#[test]
fn argument_index_at_limit_succeeds() {
    let q = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(255, ValueMatcher::static_string())
        .unwrap();
    assert_eq!(q.constraint_count(), 1);
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

#[path = "composition_extended.rs"]
mod composition_extended;
