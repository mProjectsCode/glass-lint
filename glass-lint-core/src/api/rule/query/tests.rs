use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use super::*;
use crate::api::{classification::MatchKind, rule::ValueMatcher};

// ── VarId tests ────────────────────────────────────────────────

#[test]
fn var_id_new_round_trips() {
    let id = VarId::new(42);
    assert_eq!(id.get(), 42);
}

#[test]
fn var_id_ordering_is_stable() {
    let a = VarId::new(1);
    let b = VarId::new(2);
    assert!(a < b);
}

#[test]
fn member_chain_validation_canonicalizes_display_and_path_once() {
    let query = EventQuery::member_call_rooted(" document . body ").unwrap();
    assert!(matches!(
        query.event(),
        EventSpec::MemberCall { member } if member == &SymbolPath::from("document.body")
    ));
}

// ── AnyExpr / AllExpr empty rejection ──────────────────────────

#[test]
fn any_expr_rejects_empty_branches() {
    assert_eq!(
        AnyExpr::new(vec![]),
        Err(QueryBuildError::EmptyAlternatives)
    );
}

#[test]
fn all_expr_rejects_empty_branches() {
    assert_eq!(AllExpr::new(vec![]), Err(QueryBuildError::EmptyConjunction));
}

#[test]
fn any_expr_accepts_non_empty_branches() {
    let event = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: value::ArgumentConstraints::new(),
    });
    let any = AnyExpr::new(vec![event.clone(), event]).unwrap();
    assert_eq!(any.len(), 2);
}

#[test]
fn all_expr_accepts_non_empty_branches() {
    let event = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: value::ArgumentConstraints::new(),
    });
    let all = AllExpr::new(vec![event]).unwrap();
    assert_eq!(all.len(), 1);
}

#[test]
fn expression_depth_is_bounded_before_compilation() {
    let mut nested = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: value::ArgumentConstraints::new(),
    });
    for _ in 1..limits::MAX_EXPR_DEPTH {
        nested = QueryExpr::any(AnyExpr::new(vec![nested]).unwrap());
    }

    assert!(matches!(
        AnyExpr::new(vec![nested]),
        Err(QueryBuildError::ExpressionDepthExceeded(depth))
            if depth == limits::MAX_EXPR_DEPTH + 1
    ));
}

#[test]
fn expression_child_limit_plus_one_is_rejected_at_authoring() {
    let event = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: value::ArgumentConstraints::new(),
    });
    let branches = vec![event; limits::MAX_EXPR_CHILDREN + 1];
    assert!(matches!(
        AnyExpr::new(branches),
        Err(QueryBuildError::CollectionTooLarge(
            "Any expression branches",
            257
        ))
    ));
}

// ── Construction: every EventQuery constructor → valid QueryDecl ──

#[allow(clippy::needless_pass_by_value)]
fn assert_event_query(decl: QueryDecl, expected_symbol: &str) {
    assert_eq!(decl.emission.primary_var, VarId::new(0));
    assert_eq!(decl.emission.symbol, expected_symbol);
    assert!(matches!(decl.expression().kind(), QueryExprKind::Event(_)));
}

#[allow(clippy::needless_pass_by_value)]
fn assert_any_all_query(decl: QueryDecl, expected_symbol: &str) {
    assert_eq!(decl.emission.primary_var, VarId::new(0));
    assert_eq!(decl.emission.symbol, expected_symbol);
}

#[test]
fn lowers_call_global_to_query_decl() {
    assert_event_query(
        EventQuery::call_global("fetch").unwrap().into_query(),
        "fetch",
    );
}

#[test]
fn lowers_call_heuristic_to_query_decl() {
    assert_event_query(
        EventQuery::call_heuristic("fetch").unwrap().into_query(),
        "fetch",
    );
}

#[test]
fn symbolic_query_names_are_trimmed_at_construction() {
    let global = EventQuery::call_global(" fetch ").unwrap();
    assert_eq!(
        global.identity(),
        &IdentitySpec::Global {
            name: "fetch".into()
        }
    );

    let module = EventQuery::call_module(" fs ", " readFile ").unwrap();
    assert_eq!(
        module.identity(),
        &IdentitySpec::ModuleExport {
            module: "fs".into(),
            export: "readFile".into(),
        }
    );

    let namespace = EventQuery::member_call_module(" fs ", "readFile").unwrap();
    assert_eq!(
        namespace.identity(),
        &IdentitySpec::ModuleNamespace {
            module: "fs".into()
        }
    );
}

#[test]
fn lowers_call_module_to_query_decl() {
    assert_event_query(
        EventQuery::call_module("fs", "readFile")
            .unwrap()
            .into_query(),
        "fs.readFile",
    );
}

#[test]
fn lowers_call_package_to_query_decl() {
    assert_event_query(
        EventQuery::call_package("@scope/pkg", "method")
            .unwrap()
            .into_query(),
        "@scope/pkg.method",
    );
}

#[test]
fn lowers_member_call_rooted_to_query_decl() {
    assert_event_query(
        EventQuery::member_call_rooted("document.createElement")
            .unwrap()
            .into_query(),
        "document.createElement",
    );
}

#[test]
fn lowers_member_call_heuristic_to_query_decl() {
    assert_event_query(
        EventQuery::member_call_heuristic("foo.bar")
            .unwrap()
            .into_query(),
        "foo.bar",
    );
}

#[test]
fn lowers_member_call_module_to_query_decl() {
    assert_event_query(
        EventQuery::member_call_module("module", "method")
            .unwrap()
            .into_query(),
        "module",
    );
}

#[test]
fn lowers_member_call_instance_to_query_decl() {
    assert_any_all_query(
        QueryDecl::member_call_instance("pkg", "Client", "send").unwrap(),
        "pkg.Client",
    );
}

#[test]
fn lowers_member_call_package_to_query_decl() {
    assert_event_query(
        EventQuery::member_call_package("@scope/pkg", "method")
            .unwrap()
            .into_query(),
        "@scope/pkg",
    );
}

#[test]
fn lowers_member_call_returned_to_query_decl() {
    assert_any_all_query(
        QueryDecl::member_call_returned("create", "send").unwrap(),
        "create",
    );
}

#[test]
fn lowers_member_read_rooted_to_query_decl() {
    assert_event_query(
        EventQuery::member_read_rooted("window.location")
            .unwrap()
            .into_query(),
        "window.location",
    );
}

#[test]
fn lowers_member_read_module_to_query_decl() {
    assert_event_query(
        EventQuery::member_read_module("module", "property")
            .unwrap()
            .into_query(),
        "module",
    );
}

#[test]
fn lowers_member_read_returned_to_query_decl() {
    assert_any_all_query(
        QueryDecl::member_read_returned("create", "token").unwrap(),
        "create",
    );
}

#[test]
fn lowers_member_read_package_to_query_decl() {
    assert_event_query(
        EventQuery::member_read_package("@scope/pkg", "property")
            .unwrap()
            .into_query(),
        "@scope/pkg",
    );
}

#[test]
fn lowers_import_exact_to_query_decl() {
    assert_event_query(
        EventQuery::import_exact("node:fs").unwrap().into_query(),
        "node:fs",
    );
}

#[test]
fn lowers_import_package_to_query_decl() {
    assert_event_query(
        EventQuery::import_package("@scope/pkg")
            .unwrap()
            .into_query(),
        "@scope/pkg",
    );
}

#[test]
fn lowers_string_contains_to_query_decl() {
    assert_event_query(
        EventQuery::string_contains("https://")
            .unwrap()
            .into_query(),
        "https://",
    );
}

#[test]
fn lowers_class_heuristic_to_query_decl() {
    assert_event_query(
        EventQuery::class_heuristic("Worker").unwrap().into_query(),
        "Worker",
    );
}

#[test]
fn lowers_class_module_to_query_decl() {
    assert_event_query(
        EventQuery::class_module("module", "Klass")
            .unwrap()
            .into_query(),
        "module.Klass",
    );
}

#[test]
fn lowers_constructor_global_to_query_decl() {
    assert_event_query(
        EventQuery::constructor_global("URL").unwrap().into_query(),
        "URL",
    );
}

#[test]
fn lowers_constructor_rooted_to_query_decl() {
    assert_event_query(
        EventQuery::constructor_rooted("WebAssembly.Module")
            .unwrap()
            .into_query(),
        "WebAssembly.Module",
    );
}

#[test]
fn lowers_constructor_heuristic_to_query_decl() {
    assert_event_query(
        EventQuery::constructor_heuristic("Foo")
            .unwrap()
            .into_query(),
        "Foo",
    );
}

#[test]
fn lowers_constructor_module_to_query_decl() {
    assert_event_query(
        EventQuery::constructor_module("pkg", "Klass")
            .unwrap()
            .into_query(),
        "pkg.Klass",
    );
}

#[test]
fn lowers_arg_constraints_to_query_decl() {
    let q = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg(0, ValueMatcher::static_string())
        .unwrap()
        .with_arg_static_string(1)
        .unwrap()
        .with_arg_static_strings(2, ["a", "b"])
        .unwrap()
        .with_arg_static_string_contains(3, ["token"])
        .unwrap()
        .into_query();
    match q.expression().kind() {
        QueryExprKind::Event(eq) => {
            assert_eq!(eq.constraints.len(), 4);
        }
        _ => panic!("expected Event expression"),
    }
}

#[test]
fn lowers_evidence_override_to_query_decl() {
    let q = EventQuery::call_global("fetch")
        .unwrap()
        .into_query()
        .with_evidence(MatchKind::CallArgument, "custom.fetch");
    assert_eq!(q.emission.kind, MatchKind::CallArgument);
    assert_eq!(q.emission.symbol, "custom.fetch");
}

// ── Equivalent forms produce equivalent declarations ──────────

#[test]
fn semantically_equivalent_decls_lower_equally() {
    let q_a = EventQuery::call_global("fetch").unwrap().into_query();
    let q_b = EventQuery::call_global("fetch").unwrap().into_query();
    assert_eq!(q_a, q_b);
}

// ── Diagnostic names ──────────────────────────────────────────

#[test]
fn query_expr_diagnostic_names_are_stable() {
    let event = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: value::ArgumentConstraints::new(),
    });
    assert_eq!(event.diagnostic_name(), "event");

    let any = QueryExpr::any(AnyExpr::new(vec![event.clone()]).unwrap());
    assert_eq!(any.diagnostic_name(), "any");

    let all = QueryExpr::all(AllExpr::new(vec![event]).unwrap());
    assert_eq!(all.diagnostic_name(), "all");
}

#[path = "tests_extended.rs"]
mod tests_extended;
