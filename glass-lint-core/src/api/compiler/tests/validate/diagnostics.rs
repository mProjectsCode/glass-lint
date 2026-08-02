use super::*;
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
