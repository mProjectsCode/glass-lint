use crate::api::{
    compiler::validate::error::{
        QueryCompileError, classify_lifecycle_source, is_identity_empty,
        is_valid_identity_event_pair,
    },
    rule::query::{
        EventQuery, IdentitySpec, LifecycleQuery, QueryDecl, QueryExpr, QueryExprKind,
        QueryPredicate, limits,
    },
};

/// Validate an event query for well-formedness.
pub(crate) fn validate_event_query(eq: &EventQuery) -> Result<(), QueryCompileError> {
    if !is_valid_identity_event_pair(eq.identity(), eq.event()) {
        return Err(QueryCompileError::InvalidEventPredicate {
            identity: eq.identity().diagnostic_name().to_owned(),
            event: eq.event().diagnostic_name().to_owned(),
            subject: "direct".to_string(),
            detail: "identity/event combination cannot select a semantic fact",
        });
    }
    if is_identity_empty(eq.identity()) {
        return Err(QueryCompileError::InvalidEventPredicate {
            identity: eq.identity().diagnostic_name().to_owned(),
            event: eq.event().diagnostic_name().to_owned(),
            subject: "direct".to_string(),
            detail: "identity name or pattern is empty",
        });
    }
    if !eq.constraints().is_empty() && !eq.event().supports_arguments() {
        return Err(QueryCompileError::InvalidEventPredicate {
            identity: eq.identity().diagnostic_name().to_owned(),
            event: eq.event().diagnostic_name().to_owned(),
            subject: "direct".to_string(),
            detail: "argument constraints require a call-bearing event",
        });
    }
    Ok(())
}

fn validate_lifecycle(lc: &LifecycleQuery) -> Result<(), QueryCompileError> {
    for src in lc.sources() {
        validate_event_query(src)?;

        if let Err(error) = classify_lifecycle_source(src.identity(), src.event()) {
            return Err(QueryCompileError::InvalidLifecycle {
                detail: error.detail().into(),
            });
        }
    }

    Ok(())
}

/// Consolidated structural validation pass.
///
/// Combines well-formedness, operator compatibility, boundedness, relation
/// availability, and lifecycle validation into a single recursive traversal.
pub(crate) fn pass_structure(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    check_structure(decl.expression())
}

fn check_structure(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match expr.kind() {
        QueryExprKind::Event(eq) => {
            validate_event_query(eq)?;
            let max_constraints = limits::MAX_PREDICATES_PER_ARGUMENT * limits::MAX_ARGUMENT_GROUPS;
            if eq.constraints().len() > max_constraints {
                return Err(QueryCompileError::UnboundedQuery {
                    detail: "Event query exceeds maximum argument constraint count",
                });
            }
            Ok(())
        }
        QueryExprKind::SelectEvent(_) => Ok(()),
        QueryExprKind::Require(predicate) => check_require_structure(predicate),
        QueryExprKind::Any(any) => {
            if any.len() == 0 {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "Any expression has zero branches (should have been rejected at construction)".into(),
                });
            }
            if any.len() > limits::MAX_EXPR_CHILDREN {
                return Err(QueryCompileError::UnboundedQuery {
                    detail: "Any expression exceeds maximum branch count",
                });
            }
            for branch in any.iter() {
                check_structure(branch)?;
            }
            Ok(())
        }
        QueryExprKind::All(all) => {
            if all.len() == 0 {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "All expression has zero branches (should have been rejected at construction)".into(),
                });
            }
            if all.len() > limits::MAX_EXPR_CHILDREN {
                return Err(QueryCompileError::UnboundedQuery {
                    detail: "All expression exceeds maximum branch count",
                });
            }
            for branch in all.iter() {
                check_structure(branch)?;
            }
            Ok(())
        }
        QueryExprKind::Lifecycle(lc) => validate_lifecycle(lc),
    }
}

fn check_require_structure(predicate: &QueryPredicate) -> Result<(), QueryCompileError> {
    match predicate {
        QueryPredicate::ReturnedObject { identity, .. }
            if !matches!(identity, IdentitySpec::Rooted { .. }) =>
        {
            Err(QueryCompileError::UnsupportedRelation {
                relation: "returned_object",
                detail: "returned objects require a rooted producer identity".into(),
            })
        }
        QueryPredicate::ConstructedObject { identity, .. }
            if !matches!(
                identity,
                IdentitySpec::ModuleExport { .. } | IdentitySpec::PackageModuleExport { .. }
            ) =>
        {
            Err(QueryCompileError::UnsupportedRelation {
                relation: "constructed_object",
                detail: "constructed objects require a module export identity".into(),
            })
        }
        _ => Ok(()),
    }
}
