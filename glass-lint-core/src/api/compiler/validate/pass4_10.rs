use std::collections::BTreeSet;

use super::{
    error::{QueryCompileError, is_identity_empty},
    pass1_3::validate_event_query,
};
use crate::api::rule::query::{
    EventSpec, IdentitySpec, LifecycleQuery, QueryDecl, QueryExpr, QueryExprKind, QueryPredicate,
    VarId, limits,
};

fn validate_lifecycle(lc: &LifecycleQuery) -> Result<(), QueryCompileError> {
    if lc.sources().is_empty() {
        return Err(QueryCompileError::InvalidLifecycle {
            detail: "lifecycle must have at least one source".into(),
        });
    }

    for src in lc.sources() {
        validate_event_query(src)?;

        if !matches!(
            (src.identity(), src.event()),
            (IdentitySpec::Global { .. }, EventSpec::Call)
                | (IdentitySpec::Rooted { .. }, EventSpec::MemberCall { .. })
        ) {
            return Err(QueryCompileError::InvalidLifecycle {
                detail: "lifecycle source must be a global call or rooted member call".into(),
            });
        }
    }

    if lc.condition().is_none() && lc.completion().is_none() {
        return Err(QueryCompileError::InvalidLifecycle {
            detail: "lifecycle must have at least a condition or completion".into(),
        });
    }

    Ok(())
}

// ── Consolidated entry point ─────────────────────────────────────────────

/// Validate a single [`QueryDecl`] using consolidated passes.
///
/// Runs three combined traversals instead of ten individual walks:
///
/// 1. **Structure** — well-formedness, operator compatibility, boundedness,
///    relation availability, and lifecycle structure.
/// 2. **Scope and types** — variable binding collection and type inference.
/// 3. **Correlation and evidence** — multi-event correlation and primary-
///    variable evidence projection.
///
/// Pass order prioritises structural errors before semantic errors.
pub(crate) fn validate_query_decl(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    use super::pass1_3::pass_scope_types;

    pass_structure(decl)?;
    pass_scope_types(decl)?;
    pass_correlation_evidence(decl)?;
    Ok(())
}

/// Consolidated structural validation pass.
///
/// Combines `pass_well_formedness`, `pass_operator_compatibility`,
/// `pass_boundedness`, `pass_relation_availability`, and
/// `pass_lifecycle_validation` into a single recursive traversal.
pub(crate) fn pass_structure(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    check_structure(decl.expression())
}

fn check_structure(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::Event(eq) => {
            validate_event_query(eq)?;
            // pass_relation_availability
            check_identity_not_empty(eq.identity())?;
            // pass_boundedness
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
            for b in any.iter() {
                check_structure(b)?;
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
            for b in all.iter() {
                check_structure(b)?;
            }
            Ok(())
        }
        QueryExprKind::Lifecycle(lc) => validate_lifecycle(lc),
    }
}

fn check_identity_not_empty(identity: &IdentitySpec) -> Result<(), QueryCompileError> {
    if is_identity_empty(identity) {
        return Err(QueryCompileError::UnsupportedRelation {
            relation: identity.diagnostic_name(),
            detail: format!("{} identity name is empty", identity.diagnostic_name()),
        });
    }
    Ok(())
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

/// Consolidated correlation and evidence projection pass.
///
/// Combines `pass_correlation_scope` and `pass_evidence_projection` into a
/// single recursive traversal.
pub(crate) fn pass_correlation_evidence(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    let primary = decl.emission().primary_var();
    check_correlation_evidence(decl.expression(), primary, true)
}

fn check_correlation_evidence(
    expr: &QueryExpr,
    primary: VarId,
    _is_root: bool,
) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::All(all) => {
            validate_correlated_branches(all.iter())?;
            // pass_evidence_projection
            for branch in all.iter() {
                check_correlation_evidence(branch, primary, false)?;
            }
            if !all.iter().any(|b| b.contains_var(primary)) {
                return Err(QueryCompileError::MissingBinding {
                    primary_var: primary,
                });
            }
            Ok(())
        }
        QueryExprKind::Any(any) => {
            // pass_correlation_scope recurses into Any branches
            for b in any.iter() {
                check_correlation_scope_inner(b)?;
            }
            // pass_evidence_projection: every branch must contain the primary var
            for branch in any.iter() {
                if !branch.contains_var(primary) {
                    return Err(QueryCompileError::MissingBinding {
                        primary_var: primary,
                    });
                }
            }
            Ok(())
        }
        QueryExprKind::Event(eq) => {
            if eq.var() != primary {
                return Err(QueryCompileError::MissingBinding {
                    primary_var: primary,
                });
            }
            Ok(())
        }
        QueryExprKind::SelectEvent(s) => {
            if s.bind != primary {
                return Err(QueryCompileError::MissingBinding {
                    primary_var: primary,
                });
            }
            Ok(())
        }
        QueryExprKind::Require(_) | QueryExprKind::Lifecycle(_) => Ok(()),
    }
}

// pass_correlation_scope recursion helper (for Any branches)
fn check_correlation_scope_inner(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::All(all) => {
            validate_correlated_branches(all.iter())?;
            for b in all.iter() {
                check_correlation_scope_inner(b)?;
            }
            Ok(())
        }
        QueryExprKind::Any(any) => {
            for b in any.iter() {
                check_correlation_scope_inner(b)?;
            }
            Ok(())
        }
        QueryExprKind::Event(_)
        | QueryExprKind::SelectEvent(_)
        | QueryExprKind::Require(_)
        | QueryExprKind::Lifecycle(_) => Ok(()),
    }
}

fn validate_correlated_branches<'a>(
    input: impl IntoIterator<Item = &'a QueryExpr>,
) -> Result<(), QueryCompileError> {
    let branches = input.into_iter().collect::<Vec<_>>();
    let Some(first_branch) = branches.first() else {
        return Ok(());
    };
    let first_vars: BTreeSet<VarId> = first_branch.vars().into_iter().collect();
    let has_shared = branches
        .iter()
        .skip(1)
        .any(|branch| branch.vars().iter().any(|var| first_vars.contains(var)));
    if has_shared || branches.len() <= 1 {
        Ok(())
    } else {
        Err(QueryCompileError::UncorrelatedConjunction)
    }
}
