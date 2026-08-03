use super::*;
#[test]
fn valid_global_call_passes_well_formedness() {
    let decl = EventQuery::call_global("fetch").unwrap().into_query();
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn valid_heuristic_call_passes_well_formedness() {
    let decl = EventQuery::call_heuristic("fetch").unwrap().into_query();
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn valid_rooted_member_call_passes_well_formedness() {
    let decl = EventQuery::member_call_rooted("document.createElement")
        .unwrap()
        .into_query();
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn direct_event_must_match_subject_identity() {
    let eq = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::MemberCall {
            member: SymbolPath::from("foo.bar"),
        },
        IdentitySpec::Heuristic {
            name: SmolStr::new("foo.bar"),
        },
        vec![],
    );
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::event(eq),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::MemberCall,
            symbol: "test".into(),
        },
    );
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn member_call_needs_matching_identity_name() {
    let eq = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::MemberCall {
            member: SymbolPath::from("bar"),
        },
        IdentitySpec::Heuristic {
            name: SmolStr::new("foo"),
        },
        vec![],
    );
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::event(eq),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    );
    assert_eq!(
        pass_structure(&decl),
        Err(QueryCompileError::InvalidEventPredicate {
            identity: "heuristic".into(),
            event: "member_call".into(),
            subject: "direct".into(),
            detail: "identity/event combination cannot select a semantic fact",
        })
    );
}

#[test]
fn constraints_on_non_call_event_fails() {
    let eq = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::Import,
        IdentitySpec::LiteralString {
            predicate: "node:fs".into(),
        },
        vec![ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string(),
        )],
    );
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::event(eq),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Import,
            symbol: "test".into(),
        },
    );
    assert_eq!(
        pass_structure(&decl),
        Err(QueryCompileError::InvalidEventPredicate {
            identity: "literal".into(),
            event: "import".into(),
            subject: "direct".into(),
            detail: "argument constraints require a call-bearing event",
        })
    );
}

#[test]
fn empty_identity_fails() {
    let eq = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new(""),
        },
        vec![],
    );
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::event(eq),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    );
    assert_eq!(
        pass_structure(&decl),
        Err(QueryCompileError::InvalidEventPredicate {
            identity: "global".into(),
            event: "call".into(),
            subject: "direct".into(),
            detail: "identity name or pattern is empty",
        })
    );
}

#[test]
fn duplicate_var_in_all_fails() {
    let a = global_call(0, "fetch");
    let b = global_call(0, "fetch");
    let decl = emitted(
        QueryExpr::all(AllExpr::new(vec![a, b]).unwrap()),
        0,
        MatchKind::Call,
        "fetch",
    );
    assert_eq!(
        pass_scope_types(&decl),
        Err(QueryCompileError::DuplicateBinding { var: VarId::new(0) })
    );
}

#[test]
fn unique_vars_pass_collection() {
    let a = global_call(0, "fetch");
    let b = global_call(1, "navigate");
    let decl = emitted(
        QueryExpr::all(AllExpr::new(vec![a, b]).unwrap()),
        0,
        MatchKind::Call,
        "test",
    );
    assert!(pass_scope_types(&decl).is_ok());
}

#[test]
fn emission_var_must_exist_in_expression() {
    let eq = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        vec![],
    );
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::event(eq),
        EmissionDecl {
            primary_var: VarId::new(1),
            kind: MatchKind::Call,
            symbol: "fetch".into(),
        },
    );
    assert_eq!(
        pass_correlation_evidence(&decl),
        Err(QueryCompileError::MissingBinding {
            primary_var: VarId::new(1)
        })
    );
}

#[test]
fn emission_var_exists_in_expression_passes() {
    let eq = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        vec![],
    );
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::event(eq),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "fetch".into(),
        },
    );
    assert!(pass_correlation_evidence(&decl).is_ok());
}

#[test]
fn uncorrelated_multi_event_all_fails() {
    let a = global_call(0, "fetch");
    let b = global_call(1, "navigate");
    let decl = emitted(
        QueryExpr::all(AllExpr::new(vec![a, b]).unwrap()),
        0,
        MatchKind::Call,
        "test",
    );
    assert_eq!(
        pass_correlation_evidence(&decl),
        Err(QueryCompileError::UncorrelatedConjunction)
    );
}

#[test]
fn correlated_multi_event_all_passes() {
    let a = global_call(0, "fetch");
    let b = global_call(0, "navigate");
    let decl = emitted(
        QueryExpr::all(AllExpr::new(vec![a, b]).unwrap()),
        0,
        MatchKind::Call,
        "test",
    );
    assert!(pass_correlation_evidence(&decl).is_ok());
}

#[test]
fn single_branch_all_needs_no_correlation() {
    let decl = emitted(
        QueryExpr::all(AllExpr::new(vec![global_call(0, "fetch")]).unwrap()),
        0,
        MatchKind::Call,
        "fetch",
    );
    assert!(pass_correlation_evidence(&decl).is_ok());
}

#[test]
fn bounded_query_passes_boundedness() {
    let decl = emitted(global_call(0, "fetch"), 0, MatchKind::Call, "fetch");
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn excessive_any_branches_fails_boundedness() {
    let branches: Vec<QueryExpr> = (0..1001)
        .map(|i| {
            QueryExpr::event(EventQuery::from_parts_for_test(
                VarId::new(i),
                EventSpec::Call,
                IdentitySpec::Global {
                    name: SmolStr::new(format!("f{i}")),
                },
                vec![],
            ))
        })
        .collect();
    let error = AnyExpr::new(branches).unwrap_err();
    assert!(matches!(
        error,
        crate::api::rule::QueryBuildError::CollectionTooLarge(_, 1001)
    ));
}

#[test]
fn lifecycle_source_must_be_member_call() {
    let source = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::Call,
        IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        vec![],
    );
    let lc = LifecycleQuery::from_parts_for_test(
        "test",
        vec![source],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        Some(crate::api::rule::LifecycleCompletion::configuration()),
    );
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::lifecycle(lc),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    );
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn lifecycle_source_must_be_rooted() {
    let source = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::MemberCall {
            member: SymbolPath::from("ns.method"),
        },
        IdentitySpec::ModuleNamespace {
            module: SmolStr::new("mod"),
        },
        vec![],
    );
    let lc = LifecycleQuery::from_parts_for_test(
        "test",
        vec![source],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        Some(crate::api::rule::LifecycleCompletion::configuration()),
    );
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::lifecycle(lc),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    );
    assert_eq!(
        pass_structure(&decl),
        Err(QueryCompileError::InvalidLifecycle {
            detail: "lifecycle source must be a global call or rooted member call".into(),
        })
    );
}

#[test]
fn valid_lifecycle_passes_lifecycle_validation() {
    let source = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::MemberCall {
            member: SymbolPath::from("document.createElement"),
        },
        IdentitySpec::Rooted {
            path: SymbolPath::from("document.createElement"),
        },
        vec![],
    );
    let lc = LifecycleQuery::from_parts_for_test(
        "test",
        vec![source],
        Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        Some(crate::api::rule::LifecycleCompletion::configuration()),
    );
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::lifecycle(lc),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    );
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn all_query_forms_pass_validation() {
    assert_valid_query(&EventQuery::call_global("fetch").unwrap().into_query());
    assert_valid_query(&EventQuery::call_heuristic("fetch").unwrap().into_query());
    assert_valid_query(
        &EventQuery::call_module("fs", "readFile")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(
        &EventQuery::call_package("@scope/pkg", "method")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(
        &EventQuery::member_call_rooted("document.createElement")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(
        &EventQuery::member_call_heuristic("foo.bar")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(
        &EventQuery::member_call_module("module", "method")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(&QueryDecl::member_call_instance("pkg", "Client", "send").unwrap());
    assert_valid_query(
        &EventQuery::member_call_package("@scope/pkg", "method")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(&QueryDecl::member_call_returned("create", "send").unwrap());
    assert_valid_query(
        &EventQuery::member_read_rooted("window.location")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(
        &EventQuery::member_read_module("module", "property")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(&QueryDecl::member_read_returned("create", "token").unwrap());
    assert_valid_query(
        &EventQuery::member_read_package("@scope/pkg", "property")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(&EventQuery::import_exact("node:fs").unwrap().into_query());
    assert_valid_query(
        &EventQuery::import_package("@scope/pkg")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(
        &EventQuery::string_contains("https://")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(&EventQuery::class_heuristic("Worker").unwrap().into_query());
    assert_valid_query(
        &EventQuery::class_module("module", "Klass")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(&EventQuery::constructor_global("URL").unwrap().into_query());
    assert_valid_query(
        &EventQuery::constructor_heuristic("Foo")
            .unwrap()
            .into_query(),
    );
    assert_valid_query(
        &EventQuery::constructor_module("pkg", "Klass")
            .unwrap()
            .into_query(),
    );
}
