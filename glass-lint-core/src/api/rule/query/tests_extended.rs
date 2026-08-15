use super::*;

#[test]
fn event_spec_diagnostic_names_are_stable() {
    assert_eq!(EventSpec::Call.diagnostic_name(), "call");
    assert_eq!(EventSpec::Construct.diagnostic_name(), "construct");
    assert_eq!(
        EventSpec::MemberCall {
            member: SymbolPath::from("foo")
        }
        .diagnostic_name(),
        "member_call"
    );
    assert_eq!(
        EventSpec::MemberRead {
            member: SymbolPath::from("foo")
        }
        .diagnostic_name(),
        "member_read"
    );
    assert_eq!(EventSpec::ClassReference.diagnostic_name(), "class");
    assert_eq!(EventSpec::Import.diagnostic_name(), "import");
    assert_eq!(EventSpec::StringReference.diagnostic_name(), "string");
}

#[test]
fn identity_spec_diagnostic_names_are_stable() {
    assert_eq!(
        IdentitySpec::Global {
            name: SmolStr::new("f")
        }
        .diagnostic_name(),
        "global"
    );
    assert_eq!(
        IdentitySpec::Heuristic {
            name: SmolStr::new("f")
        }
        .diagnostic_name(),
        "heuristic"
    );
    assert_eq!(
        IdentitySpec::ModuleExport {
            module: SmolStr::new("m"),
            export: SmolStr::new("e")
        }
        .diagnostic_name(),
        "module_export"
    );
    assert_eq!(
        IdentitySpec::Rooted {
            path: SymbolPath::from("a.b")
        }
        .diagnostic_name(),
        "rooted"
    );
    assert_eq!(
        IdentitySpec::LiteralString {
            predicate: "s".into()
        }
        .diagnostic_name(),
        "literal"
    );
}

// ── Display and plan summary ──────────────────────────────────

#[test]
fn query_expr_display_shapes_are_compact() {
    let event = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: value::ArgumentConstraints::new(),
    });
    let text = format!("{event}");
    assert!(text.contains("select"));
    assert!(text.contains("$0"));
    assert!(text.contains("call"));
    assert!(text.contains("global"));
}

#[test]
fn any_display_shows_branches() {
    let event = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: value::ArgumentConstraints::new(),
    });
    let any = QueryExpr::any(AnyExpr::new(vec![event]).unwrap());
    let text = format!("{any}");
    assert!(text.starts_with("any ["));
    assert!(text.ends_with(']'));
}

#[test]
fn query_decl_display_includes_symbol() {
    let q = EventQuery::call_global("fetch").unwrap().into_query();
    let text = format!("{q}");
    assert!(text.contains("fetch"));
}

#[test]
fn query_explanation_includes_identity_and_argument_constraints() {
    let decl = EventQuery::call_global("fetch")
        .unwrap()
        .with_arg_object_keys(0, ["url"])
        .unwrap()
        .into_query();

    assert_eq!(
        decl.explanation(),
        "Emit `fetch` when a call to the global `fetch` with argument 0 matches an object with keys `url`."
    );
}

#[test]
fn queries_lower_correctly() {
    let queries = [
        EventQuery::call_global("fetch").unwrap().into_query(),
        EventQuery::member_read_rooted("window.location")
            .unwrap()
            .into_query(),
    ];
    assert_eq!(queries.len(), 2);
}

// ── VarId collection ──────────────────────────────────────────

#[test]
fn event_query_vars_contains_one() {
    let event = QueryExpr::event(EventQuery {
        var: VarId::new(5),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("f"),
        },
        constraints: value::ArgumentConstraints::new(),
    });
    assert_eq!(event.vars(), vec![VarId::new(5)]);
}

#[test]
fn any_query_vars_collects_all_branch_vars() {
    let a = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("f"),
        },
        constraints: value::ArgumentConstraints::new(),
    });
    let b = QueryExpr::event(EventQuery {
        var: VarId::new(1),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("g"),
        },
        constraints: value::ArgumentConstraints::new(),
    });
    let any = QueryExpr::any(AnyExpr::new(vec![a, b]).unwrap());
    let vars = any.vars();
    assert_eq!(vars.len(), 2);
    assert!(vars.contains(&VarId::new(0)));
    assert!(vars.contains(&VarId::new(1)));
}
