use super::*;
#[test]
fn returned_object_with_non_rooted_identity_fails_at_structure() {
    let event = EventQuery::member_call_rooted("document.createElement").unwrap();
    let branches = vec![
        QueryExpr::event(event),
        QueryExpr::require(QueryPredicate::ReturnedObject {
            bind: VarId::new(1),
            identity: IdentitySpec::Global {
                name: SmolStr::new("create"),
            },
        }),
    ];
    let all_expr = AllExpr::new(branches).unwrap();
    let decl = QueryDecl {
        expression: QueryExpr::all(all_expr),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::MemberCall,
            symbol: "test".into(),
        },
    };
    let result = validate_query_decl(&decl);
    assert!(
        matches!(
            result,
            Err(QueryCompileError::UnsupportedRelation {
                relation: "returned_object",
                ..
            })
        ),
        "expected UnsupportedRelation for returned_object with global identity, got: {result:?}"
    );
}

#[test]
fn constructed_object_with_non_module_export_identity_fails_at_structure() {
    let event = EventQuery::call_global("create").unwrap();
    let branches = vec![
        QueryExpr::event(event),
        QueryExpr::require(QueryPredicate::ConstructedObject {
            bind: VarId::new(1),
            identity: IdentitySpec::Global {
                name: SmolStr::new("create"),
            },
        }),
    ];
    let all_expr = AllExpr::new(branches).unwrap();
    let decl = QueryDecl {
        expression: QueryExpr::all(all_expr),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
    let result = validate_query_decl(&decl);
    assert!(
        matches!(
            result,
            Err(QueryCompileError::UnsupportedRelation {
                relation: "constructed_object",
                ..
            })
        ),
        "expected UnsupportedRelation for constructed_object with global identity, got: {result:?}"
    );
}

#[test]
fn valid_returned_object_with_rooted_identity_passes() {
    let event = EventQuery::member_call_rooted("document.createElement").unwrap();
    let branches = vec![
        QueryExpr::event(event),
        QueryExpr::require(QueryPredicate::ReturnedObject {
            bind: VarId::new(1),
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from("element"),
            },
        }),
        QueryExpr::require(QueryPredicate::MemberSubject {
            event: VarId::new(0),
            object: VarId::new(1),
        }),
    ];
    let all_expr = AllExpr::new(branches).unwrap();
    let decl = QueryDecl {
        expression: QueryExpr::all(all_expr),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::MemberCall,
            symbol: "test".into(),
        },
    };
    assert_valid_query(&decl);
}

#[test]
fn valid_constructed_object_with_module_export_identity_passes() {
    let event = EventQuery::member_call_rooted("someLib.createWidget").unwrap();
    let branches = vec![
        QueryExpr::event(event),
        QueryExpr::require(QueryPredicate::ConstructedObject {
            bind: VarId::new(1),
            identity: IdentitySpec::ModuleExport {
                module: SmolStr::new("some-lib"),
                export: SmolStr::new("Widget"),
            },
        }),
        QueryExpr::require(QueryPredicate::MemberSubject {
            event: VarId::new(0),
            object: VarId::new(1),
        }),
    ];
    let all_expr = AllExpr::new(branches).unwrap();
    let decl = QueryDecl {
        expression: QueryExpr::all(all_expr),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::MemberCall,
            symbol: "test".into(),
        },
    };
    assert_valid_query(&decl);
}
