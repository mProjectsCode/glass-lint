use super::*;
#[test]
fn relation_availability_passes_for_valid_global() {
    let decl = emitted(global_call(0, "fetch"), 0, MatchKind::Call, "fetch");
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn well_formedness_error_precedes_projection_error() {
    let eq = EventQuery::from_parts_for_test(
        VarId::new(0),
        EventSpec::Import,
        IdentitySpec::Global {
            name: SmolStr::new("fs"),
        },
        vec![],
    );
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::event(eq),
        EmissionDecl {
            primary_var: VarId::new(1),
            kind: MatchKind::Import,
            symbol: "fs".into(),
        },
    );
    let result = validate_query_decl(&decl);
    assert_eq!(
        result,
        Err(QueryCompileError::InvalidEventPredicate {
            identity: "global".into(),
            event: "import".into(),
            subject: "direct".into(),
            detail: "identity/event combination cannot select a semantic fact",
        })
    );
}

#[test]
fn any_with_incompatible_branch_types_fails() {
    let branch_a = EventQuery::call_global("fetch").unwrap().into_query();
    let branch_b = EventQuery::member_call_rooted("document.createElement")
        .unwrap()
        .into_query();
    let result = QueryDecl::any([Ok(branch_a), Ok(branch_b)]);
    assert!(matches!(
        result,
        Err(crate::api::rule::QueryBuildError::EvidenceProjection)
    ));
}

#[test]
fn any_with_compatible_branch_types_passes() {
    let branch_a = EventQuery::call_global("fetch").unwrap().into_query();
    let branch_b = EventQuery::call_global("navigate").unwrap().into_query();
    let query = QueryDecl::any_with_evidence([Ok(branch_a), Ok(branch_b)], "test").unwrap();
    assert!(pass_scope_types(&query).is_ok());
}

#[test]
fn incompatible_branch_output_has_correct_diagnostic_name() {
    let err = QueryCompileError::IncompatibleBranchOutput {
        var: VarId::new(0),
        type_a: "call_event",
        type_b: "member_event",
    };
    assert_eq!(err.diagnostic_name(), "incompatible_branch_output");
}

#[test]
fn reference_before_binding_fails() {
    let branches = vec![
        QueryExpr::require(QueryPredicate::EventKind {
            event: VarId::new(0),
            expected: EventSpec::Call,
        }),
        QueryExpr::select_event(VarId::new(0)),
    ];
    let all_expr = AllExpr::new(branches).unwrap();
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::all(all_expr),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    );
    let result = pass_scope_types(&decl);
    assert!(
        matches!(
            result,
            Err(QueryCompileError::MissingBinding { primary_var }) if primary_var == VarId::new(0)
        ),
        "expected MissingBinding for $0 referenced before binding, got: {result:?}"
    );
}

#[test]
fn reference_after_binding_passes() {
    let branches = vec![
        QueryExpr::select_event(VarId::new(0)),
        QueryExpr::require(QueryPredicate::EventKind {
            event: VarId::new(0),
            expected: EventSpec::Call,
        }),
    ];
    let all_expr = AllExpr::new(branches).unwrap();
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::all(all_expr),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    );
    assert!(pass_scope_types(&decl).is_ok());
}

#[test]
fn type_mismatch_between_event_and_object_fails() {
    let branches = vec![
        QueryExpr::require(QueryPredicate::ReturnedObject {
            bind: VarId::new(0),
            identity: IdentitySpec::Global {
                name: SmolStr::new("create"),
            },
        }),
        QueryExpr::require(QueryPredicate::EventKind {
            event: VarId::new(0),
            expected: EventSpec::Call,
        }),
    ];
    let all_expr = AllExpr::new(branches).unwrap();
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::all(all_expr),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    );
    let result = pass_scope_types(&decl);
    assert!(
        matches!(
            result,
            Err(QueryCompileError::TypeMismatch { var, .. }) if var == VarId::new(0)
        ),
        "expected TypeMismatch for $0 (object vs call), got: {result:?}"
    );
}

#[test]
fn emission_from_object_var_fails() {
    let branches = vec![QueryExpr::require(QueryPredicate::ReturnedObject {
        bind: VarId::new(0),
        identity: IdentitySpec::Global {
            name: SmolStr::new("create"),
        },
    })];
    let all_expr = AllExpr::new(branches).unwrap();
    let decl = QueryDecl::from_parts_for_test(
        QueryExpr::all(all_expr),
        EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    );
    let result = pass_scope_types(&decl);
    assert!(
        matches!(
            result,
            Err(QueryCompileError::UnavailablePrimaryLocation { var }) if var == VarId::new(0)
        ),
        "expected UnavailablePrimaryLocation for $0 (Object), got: {result:?}"
    );
}
