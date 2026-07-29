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
        AllExpr, AnyExpr, Category, Confidence, EventQuery, EventSpec, IdentitySpec,
        LifecycleQuery, QueryBuildError, QueryDecl, QueryExpr, Rule, SubjectSpec, ValueMatcher,
        VarId,
    },
};

// ── Helpers ────────────────────────────────────────────────────────────

/// Build a minimal rule with one query, ready for catalog compilation.
fn compile_rule(
    id: &str,
    query: QueryDecl,
) -> Result<RuleCatalog, glass_lint_core::ProviderCatalogError> {
    let rule = Rule::builder(id)
        .description("test")
        .category(Category::new("test").unwrap())
        .severity(glass_lint_core::Severity::Info)
        .confidence(Confidence::High)
        .query(query)
        .build()
        .unwrap();
    RuleCatalog::new("test", vec![rule])
}

/// Build a tracked-template query whose emission can be reused in Any/All.
/// Using `call_global` gives MatchKind::Call without naming the type.
fn template_query() -> QueryDecl {
    EventQuery::call_global("template").into_query()
}

/// Create an EventQuery with the given var and global identity.
fn event_var(var: u32, name: &str) -> EventQuery {
    EventQuery {
        var: VarId::new(var),
        event: EventSpec::Call,
        identity: IdentitySpec::Global { name: name.into() },
        subject: SubjectSpec::Direct,
        constraints: vec![],
    }
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
#[ignore = "Package 0 regression probe — Any branch-local scopes not implemented"]
fn any_branches_compile_through_rule_catalog() {
    let branch_a = QueryExpr::Event(event_var(0, "fetch"));
    let branch_b = QueryExpr::Event(event_var(0, "navigate"));
    let any_expr = AnyExpr::new(vec![branch_a, branch_b]).unwrap();
    let mut query = template_query();
    query.emission.primary_var = VarId::new(0);
    query.emission.symbol = "test.any".into();
    query.expression = QueryExpr::Any(any_expr);

    let result = compile_rule("test.any", query);
    assert!(
        result.is_ok(),
        "Any with alpha-aligned variables should compile through RuleCatalog: {result:?}"
    );
}

// ── Test 2: Any requires primary evidence on every branch ───────────────
//
// An Any whose emission primary variable is unavailable on one branch must
// produce a stable structured compile error.
//
// Currently passes validation because pass_evidence_projection only checks
// that the primary var exists somewhere in the tree (Package 3 will fix).

#[test]
#[ignore = "Package 0 regression probe — Any evidence check not per-branch"]
fn any_requires_primary_evidence_on_every_branch() {
    let branch_a = QueryExpr::Event(event_var(0, "fetch"));
    let branch_b = QueryExpr::Event(event_var(1, "navigate"));
    let any_expr = AnyExpr::new(vec![branch_a, branch_b]).unwrap();
    let mut query = template_query();
    // primary_var = VarId(0) exists in branch_a but not branch_b.
    query.emission.primary_var = VarId::new(0);
    query.emission.symbol = "test.any".into();
    query.expression = QueryExpr::Any(any_expr);

    let result = compile_rule("test.any-evidence", query);
    assert!(
        result.is_err(),
        "Any whose emission is unavailable on every branch should be rejected: {result:?}"
    );
}

// ── Test 3: Same-event All compiles through the catalog ─────────────────
//
// Two compatible constraints on one selected event must compile through
// RuleCatalog::new, producing a plan that includes both predicates.
//
// Currently fails because pass_variable_collection treats All branches as
// one flat scope and rejects same-var references as duplicates (Package 3).

#[test]
#[ignore = "Package 0 regression probe — same-event All var scoping not implemented"]
fn same_event_all_compiles_through_rule_catalog() {
    let branch_a = QueryExpr::Event(event_var(0, "fetch"));
    let branch_b = QueryExpr::Event(event_var(0, "fetch"));
    let all_expr = AllExpr::new(vec![branch_a, branch_b]).unwrap();
    let mut query = template_query();
    query.emission.primary_var = VarId::new(0);
    query.emission.symbol = "fetch".into();
    query.expression = QueryExpr::All(all_expr);

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
    let branch_a = QueryExpr::Event(event_var(0, "fetch"));
    let branch_b = QueryExpr::Event(event_var(1, "navigate"));
    let all_expr = AllExpr::new(vec![branch_a, branch_b]).unwrap();
    let mut query = template_query();
    query.emission.primary_var = VarId::new(0);
    query.emission.symbol = "test".into();
    query.expression = QueryExpr::All(all_expr);

    let result = compile_rule("test.uncorrelated", query);
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("uncorrelated_conjunction"),
        "Uncorrelated All should report 'uncorrelated_conjunction', got: {msg}"
    );
}

// ── Test 5: Contradictory same-event All fails at compilation ───────────
//
// Mutually exclusive constraints on the same event or argument must produce
// a structured contradiction error.
//
// Currently the query compiles successfully because contradiction detection
// is not implemented in the validator (Package 4 will add this).

#[test]
#[ignore = "Package 0 regression probe — contradiction detection not implemented"]
fn contradictory_same_event_all_fails_at_compilation() {
    // One event query with two contradictory argument constraints:
    // argument 0 must equal "a" AND argument 0 must equal "b".
    // This is statically contradictory.
    let query = EventQuery::call_global("fetch")
        .with_arg(0, ValueMatcher::static_string().equals("a"))
        .with_arg(0, ValueMatcher::static_string().equals("b"))
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
#[ignore = "Package 0 regression probe — lifecycle source scoping not implemented"]
fn multiple_lifecycle_sources_compile() {
    use glass_lint_core::rules::FlowCompletion;

    let src_a = EventQuery {
        var: VarId::new(0),
        event: EventSpec::MemberCall {
            member: "document.createElement".into(),
        },
        identity: IdentitySpec::Rooted {
            path: "document.createElement".into(),
        },
        subject: SubjectSpec::Direct,
        constraints: vec![],
    };
    // Second source uses the same object variable — valid Any-of-source semantics
    // where either independently valid source can start the lifecycle.
    let src_b = EventQuery {
        var: VarId::new(0),
        event: EventSpec::MemberCall {
            member: "document.createTextNode".into(),
        },
        identity: IdentitySpec::Rooted {
            path: "document.createTextNode".into(),
        },
        subject: SubjectSpec::Direct,
        constraints: vec![],
    };
    let lifecycle = LifecycleQuery {
        sources: vec![src_a, src_b],
        condition: Some(glass_lint_core::rules::FlowCondition::event(
            glass_lint_core::rules::ObjectEventMatcher::property_write(
                "type",
                glass_lint_core::rules::ValueMatcher::any_value(),
            ),
        )),
        completion: Some(FlowCompletion::configuration()),
    };
    let mut query = template_query();
    query.emission.primary_var = VarId::new(0);
    query.emission.symbol = "test.lifecycle".into();
    query.expression = QueryExpr::Lifecycle(lifecycle);

    let result = compile_rule("test.lifecycle", query);
    assert!(
        result.is_ok(),
        "Lifecycle with multiple sources should compile through RuleBuilder::query: {result:?}"
    );
}

// ── Test 7: Invalid authoring input never panics ────────────────────────
//
// Every text/index/collection constructor returns a structured error rather
// than panicking via assert! or expect.
//
// Currently several constructors use assert! which panics on invalid input
// (Package 2 will make them fallible).

#[test]
#[ignore = "Package 0 regression probe — constructors panic on invalid input"]
fn invalid_authoring_input_never_panics() {
    // Empty global call name
    let empty_name = catch_unwind(|| EventQuery::call_global(""));
    assert!(
        empty_name.is_ok(),
        "EventQuery::call_global with empty name must not panic"
    );

    // Empty heuristic call name
    let empty_heuristic = catch_unwind(|| EventQuery::call_heuristic(""));
    assert!(
        empty_heuristic.is_ok(),
        "EventQuery::call_heuristic with empty name must not panic"
    );

    // Empty module string
    let empty_module = catch_unwind(|| EventQuery::call_module("", "export"));
    assert!(
        empty_module.is_ok(),
        "EventQuery::call_module with empty module must not panic"
    );

    // Empty export string
    let empty_export = catch_unwind(|| EventQuery::call_module("fs", ""));
    assert!(
        empty_export.is_ok(),
        "EventQuery::call_module with empty export must not panic"
    );

    // Malformed chain (double dot)
    let malformed_chain = catch_unwind(|| EventQuery::member_call_rooted("a..b"));
    assert!(
        malformed_chain.is_ok(),
        "EventQuery::member_call_rooted with '..' must not panic"
    );

    // Malformed chain (trailing dot)
    let trailing_dot = catch_unwind(|| EventQuery::member_call_rooted("a.b."));
    assert!(
        trailing_dot.is_ok(),
        "EventQuery::member_call_rooted with trailing dot must not panic"
    );

    // Empty import
    let empty_import = catch_unwind(|| EventQuery::import_exact(""));
    assert!(
        empty_import.is_ok(),
        "EventQuery::import_exact with empty module must not panic"
    );

    // Empty string reference
    let empty_string = catch_unwind(|| EventQuery::string_contains(""));
    assert!(
        empty_string.is_ok(),
        "EventQuery::string_contains with empty value must not panic"
    );

    // Empty class name
    let empty_class_heuristic = catch_unwind(|| EventQuery::class_heuristic(""));
    assert!(
        empty_class_heuristic.is_ok(),
        "EventQuery::class_heuristic with empty name must not panic"
    );

    // Empty constructor
    let empty_constructor_global = catch_unwind(|| EventQuery::constructor_global(""));
    assert!(
        empty_constructor_global.is_ok(),
        "EventQuery::constructor_global with empty name must not panic"
    );

    // Empty AnyExpr
    let empty_any = AnyExpr::new(vec![]);
    assert_eq!(empty_any, Err(QueryBuildError::EmptyAlternatives));

    // Empty AllExpr
    let empty_all = AllExpr::new(vec![]);
    assert_eq!(empty_all, Err(QueryBuildError::EmptyConjunction));
}

// ── Test 8: Event-only modifiers reject non-event expressions ───────────
//
// Applying with_arg (an event-only modifier) to an Any, All, or Lifecycle
// expression must return a structured error rather than silently returning
// the original query unchanged.
//
// Currently with_arg uses `if let QueryExpr::Event(...)` to silently skip
// non-Event expressions (Package 2 will make this an error).

#[test]
#[ignore = "Package 0 regression probe — modifiers silently ignore non-Event expressions"]
fn query_modifiers_do_not_silently_ignore_non_event_expressions() {
    let branch = QueryExpr::Event(event_var(0, "fetch"));
    let any_expr = AnyExpr::new(vec![branch]).unwrap();
    let mut query = template_query();
    query.emission.primary_var = VarId::new(0);
    query.emission.symbol = "test".into();
    query.expression = QueryExpr::Any(any_expr);

    // Apply with_arg to the Any query.  Currently this silently returns the
    // query unchanged because with_arg only matches QueryExpr::Event.
    let modified = query.with_arg(0, ValueMatcher::static_string());
    assert!(
        !matches!(modified.expression, QueryExpr::Any(_)),
        "with_arg on Any should not silently return the query unchanged"
    );
}
