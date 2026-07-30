use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::{
    classification::MatchKind,
    compiler::validate::{
        QueryCompileError, pass_correlation_evidence, pass_scope_types, pass_structure,
        validate_query_decl,
    },
    rule::{
        ArgumentConstraint, QueryDecl, ValueMatcher,
        query::{
            AllExpr, AnyExpr, EmissionDecl, EventQuery, EventSpec, IdentitySpec, LifecycleQuery,
            QueryExpr, QueryPredicate, VarId,
        },
    },
};

fn assert_valid_query(decl: &QueryDecl) {
    if let Err(e) = validate_query_decl(decl) {
        panic!("query validation failed: {} ({})", e, e.diagnostic_name());
    }
}

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
    let eq = EventQuery {
        var: VarId::new(0),
        event: EventSpec::MemberCall {
            member: SymbolPath::from("foo.bar"),
        },
        identity: IdentitySpec::Heuristic {
            name: SmolStr::new("foo.bar"),
        },
        constraints: vec![],
    };
    let decl = QueryDecl {
        expression: QueryExpr::event(eq),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::MemberCall,
            symbol: "test".into(),
        },
    };
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn member_call_needs_matching_identity_name() {
    let eq = EventQuery {
        var: VarId::new(0),
        event: EventSpec::MemberCall {
            member: SymbolPath::from("bar"),
        },
        identity: IdentitySpec::Heuristic {
            name: SmolStr::new("foo"),
        },
        constraints: vec![],
    };
    let decl = QueryDecl {
        expression: QueryExpr::event(eq),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
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
    let eq = EventQuery {
        var: VarId::new(0),
        event: EventSpec::Import,
        identity: IdentitySpec::LiteralString {
            predicate: "node:fs".into(),
        },
        constraints: vec![ArgumentConstraint::new(
            crate::api::rule::ArgumentIndex::new_unchecked(0),
            ValueMatcher::static_string(),
        )],
    };
    let decl = QueryDecl {
        expression: QueryExpr::event(eq),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Import,
            symbol: "test".into(),
        },
    };
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
    let eq = EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new(""),
        },
        constraints: vec![],
    };
    let decl = QueryDecl {
        expression: QueryExpr::event(eq),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
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
    let a = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    });
    let b = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    });
    let decl = QueryDecl {
        expression: QueryExpr::all(AllExpr::new(vec![a, b]).unwrap()),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "fetch".into(),
        },
    };
    assert_eq!(
        pass_scope_types(&decl),
        Err(QueryCompileError::DuplicateBinding { var: VarId::new(0) })
    );
}

#[test]
fn unique_vars_pass_collection() {
    let a = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    });
    let b = QueryExpr::event(EventQuery {
        var: VarId::new(1),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("navigate"),
        },
        constraints: vec![],
    });
    let decl = QueryDecl {
        expression: QueryExpr::all(AllExpr::new(vec![a, b]).unwrap()),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
    assert!(pass_scope_types(&decl).is_ok());
}

#[test]
fn emission_var_must_exist_in_expression() {
    let eq = EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    };
    let decl = QueryDecl {
        expression: QueryExpr::event(eq),
        emission: EmissionDecl {
            primary_var: VarId::new(1),
            kind: MatchKind::Call,
            symbol: "fetch".into(),
        },
    };
    assert_eq!(
        pass_correlation_evidence(&decl),
        Err(QueryCompileError::MissingBinding {
            primary_var: VarId::new(1)
        })
    );
}

#[test]
fn emission_var_exists_in_expression_passes() {
    let eq = EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    };
    let decl = QueryDecl {
        expression: QueryExpr::event(eq),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "fetch".into(),
        },
    };
    assert!(pass_correlation_evidence(&decl).is_ok());
}

#[test]
fn uncorrelated_multi_event_all_fails() {
    let a = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    });
    let b = QueryExpr::event(EventQuery {
        var: VarId::new(1),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("navigate"),
        },
        constraints: vec![],
    });
    let decl = QueryDecl {
        expression: QueryExpr::all(AllExpr::new(vec![a, b]).unwrap()),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
    assert_eq!(
        pass_correlation_evidence(&decl),
        Err(QueryCompileError::UncorrelatedConjunction)
    );
}

#[test]
fn correlated_multi_event_all_passes() {
    let a = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    });
    let b = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("navigate"),
        },
        constraints: vec![],
    });
    let decl = QueryDecl {
        expression: QueryExpr::all(AllExpr::new(vec![a, b]).unwrap()),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
    assert!(pass_correlation_evidence(&decl).is_ok());
}

#[test]
fn single_branch_all_needs_no_correlation() {
    let a = QueryExpr::event(EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    });
    let decl = QueryDecl {
        expression: QueryExpr::all(AllExpr::new(vec![a]).unwrap()),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "fetch".into(),
        },
    };
    assert!(pass_correlation_evidence(&decl).is_ok());
}

#[test]
fn bounded_query_passes_boundedness() {
    let eq = EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    };
    let decl = QueryDecl {
        expression: QueryExpr::event(eq),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "fetch".into(),
        },
    };
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn excessive_any_branches_fails_boundedness() {
    let branches: Vec<QueryExpr> = (0..1001)
        .map(|i| {
            QueryExpr::event(EventQuery {
                var: VarId::new(i),
                event: EventSpec::Call,
                identity: IdentitySpec::Global {
                    name: SmolStr::new(format!("f{i}")),
                },
                constraints: vec![],
            })
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
    let source = EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    };
    let lc = LifecycleQuery {
        symbol: "test".into(),
        sources: vec![source],
        condition: Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        completion: Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
    };
    let decl = QueryDecl {
        expression: QueryExpr::lifecycle(lc),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
    assert_eq!(
        pass_structure(&decl),
        Err(QueryCompileError::InvalidLifecycle {
            detail: "lifecycle source event must be a member call".into(),
        })
    );
}

#[test]
fn lifecycle_source_must_be_rooted() {
    let source = EventQuery {
        var: VarId::new(0),
        event: EventSpec::MemberCall {
            member: SymbolPath::from("ns.method"),
        },
        identity: IdentitySpec::ModuleNamespace {
            module: SmolStr::new("mod"),
        },
        constraints: vec![],
    };
    let lc = LifecycleQuery {
        symbol: "test".into(),
        sources: vec![source],
        condition: Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        completion: Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
    };
    let decl = QueryDecl {
        expression: QueryExpr::lifecycle(lc),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
    assert_eq!(
        pass_structure(&decl),
        Err(QueryCompileError::InvalidLifecycle {
            detail: "lifecycle source identity must be rooted".into(),
        })
    );
}

#[test]
fn valid_lifecycle_passes_lifecycle_validation() {
    let source = EventQuery {
        var: VarId::new(0),
        event: EventSpec::MemberCall {
            member: SymbolPath::from("document.createElement"),
        },
        identity: IdentitySpec::Rooted {
            path: SymbolPath::from("document.createElement"),
        },
        constraints: vec![],
    };
    let lc = LifecycleQuery {
        symbol: "test".into(),
        sources: vec![source],
        condition: Some(
            crate::api::rule::LifecycleCondition::event(
                crate::api::rule::LifecycleEvent::property_write("type", ValueMatcher::any_value()),
            )
            .unwrap(),
        ),
        completion: Some(crate::api::rule::LifecycleCompletion::configuration().unwrap()),
    };
    let decl = QueryDecl {
        expression: QueryExpr::lifecycle(lc),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
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

#[test]
fn duplicate_binding_error_has_correct_diagnostic_name() {
    let err = QueryCompileError::DuplicateBinding { var: VarId::new(0) };
    assert_eq!(err.diagnostic_name(), "duplicate_binding");
}

#[test]
fn missing_binding_error_has_correct_diagnostic_name() {
    let err = QueryCompileError::MissingBinding {
        primary_var: VarId::new(0),
    };
    assert_eq!(err.diagnostic_name(), "missing_binding");
}

#[test]
fn type_mismatch_error_has_correct_diagnostic_name() {
    let err = QueryCompileError::TypeMismatch {
        var: VarId::new(0),
        expected: "event",
        actual: "object",
    };
    assert_eq!(err.diagnostic_name(), "type_mismatch");
}

#[test]
fn invalid_event_predicate_error_has_correct_diagnostic_name() {
    let err = QueryCompileError::InvalidEventPredicate {
        identity: "global".into(),
        event: "call".into(),
        subject: "direct".into(),
        detail: "test",
    };
    assert_eq!(err.diagnostic_name(), "invalid_event_predicate");
}

#[test]
fn unsupported_relation_error_has_correct_diagnostic_name() {
    let err = QueryCompileError::UnsupportedRelation {
        relation: "global",
        detail: "test".into(),
    };
    assert_eq!(err.diagnostic_name(), "unsupported_relation");
}

#[test]
fn uncorrelated_conjunction_has_correct_diagnostic_name() {
    let err = QueryCompileError::UncorrelatedConjunction;
    assert_eq!(err.diagnostic_name(), "uncorrelated_conjunction");
}

#[test]
fn unavailable_primary_location_has_correct_diagnostic_name() {
    let err = QueryCompileError::UnavailablePrimaryLocation { var: VarId::new(0) };
    assert_eq!(err.diagnostic_name(), "unavailable_primary_location");
}

#[test]
fn invalid_lifecycle_error_has_correct_diagnostic_name() {
    let err = QueryCompileError::InvalidLifecycle {
        detail: "test".into(),
    };
    assert_eq!(err.diagnostic_name(), "invalid_lifecycle");
}

#[test]
fn unbounded_query_error_has_correct_diagnostic_name() {
    let err = QueryCompileError::UnboundedQuery { detail: "test" };
    assert_eq!(err.diagnostic_name(), "unbounded_query");
}

#[test]
fn internal_invariant_error_has_correct_diagnostic_name() {
    let err = QueryCompileError::InternalInvariant {
        detail: "test".into(),
    };
    assert_eq!(err.diagnostic_name(), "internal_invariant");
}

#[test]
fn query_compile_error_displays_meaningful_message() {
    let err = QueryCompileError::MissingBinding {
        primary_var: VarId::new(0),
    };
    let msg = err.to_string();
    assert!(msg.contains("$0"));
    assert!(msg.contains("not bound"));
}

#[test]
fn invalid_event_predicate_displays_details() {
    let err = QueryCompileError::InvalidEventPredicate {
        identity: "global".into(),
        event: "call".into(),
        subject: "direct".into(),
        detail: "test reason",
    };
    let msg = err.to_string();
    assert!(msg.contains("global"));
    assert!(msg.contains("test reason"));
}

#[test]
fn any_with_empty_branches_rejected() {
    let err = AnyExpr::new(vec![]).unwrap_err();
    assert_eq!(
        err,
        crate::api::rule::query::QueryBuildError::EmptyAlternatives
    );
}

#[test]
fn all_with_empty_branches_rejected() {
    let err = AllExpr::new(vec![]).unwrap_err();
    assert_eq!(
        err,
        crate::api::rule::query::QueryBuildError::EmptyConjunction
    );
}

#[test]
fn relation_availability_passes_for_valid_global() {
    let eq = EventQuery {
        var: VarId::new(0),
        event: EventSpec::Call,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fetch"),
        },
        constraints: vec![],
    };
    let decl = QueryDecl {
        expression: QueryExpr::event(eq),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "fetch".into(),
        },
    };
    assert!(pass_structure(&decl).is_ok());
}

#[test]
fn well_formedness_error_precedes_projection_error() {
    let eq = EventQuery {
        var: VarId::new(0),
        event: EventSpec::Import,
        identity: IdentitySpec::Global {
            name: SmolStr::new("fs"),
        },
        constraints: vec![],
    };
    let decl = QueryDecl {
        expression: QueryExpr::event(eq),
        emission: EmissionDecl {
            primary_var: VarId::new(1),
            kind: MatchKind::Import,
            symbol: "fs".into(),
        },
    };
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
    let query = QueryDecl::any([Ok(branch_a), Ok(branch_b)]).unwrap();
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
    let decl = QueryDecl {
        expression: QueryExpr::all(all_expr),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
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
    let decl = QueryDecl {
        expression: QueryExpr::all(all_expr),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
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
    let decl = QueryDecl {
        expression: QueryExpr::all(all_expr),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
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
    let decl = QueryDecl {
        expression: QueryExpr::all(all_expr),
        emission: EmissionDecl {
            primary_var: VarId::new(0),
            kind: MatchKind::Call,
            symbol: "test".into(),
        },
    };
    let result = pass_scope_types(&decl);
    assert!(
        matches!(
            result,
            Err(QueryCompileError::UnavailablePrimaryLocation { var }) if var == VarId::new(0)
        ),
        "expected UnavailablePrimaryLocation for $0 (Object), got: {result:?}"
    );
}

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
