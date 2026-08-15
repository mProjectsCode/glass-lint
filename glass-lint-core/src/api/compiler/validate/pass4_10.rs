use std::collections::BTreeSet;

use super::{
    error::{QueryCompileError, classify_lifecycle_source, is_identity_empty},
    pass1_3::validate_event_query,
};
use crate::api::rule::query::{
    IdentitySpec, LifecycleQuery, QueryDecl, QueryExpr, QueryExprKind, QueryPredicate,
    QueryShapeFacts, VarId, limits,
};

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
    match expr.kind() {
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
    check_correlation_evidence(decl.expression(), primary, EvidenceScope::Primary)
}

#[derive(Clone, Copy)]
enum EvidenceScope {
    Primary,
    Nested,
}

impl EvidenceScope {
    fn checks_primary(self) -> bool {
        matches!(self, Self::Primary)
    }

    fn nested() -> Self {
        Self::Nested
    }
}

fn check_correlation_evidence(
    expr: &QueryExpr,
    primary: VarId,
    scope: EvidenceScope,
) -> Result<(), QueryCompileError> {
    match expr.kind() {
        QueryExprKind::All(all) => {
            let branch_facts: Vec<QueryShapeFacts> =
                all.iter().map(QueryExpr::shape_facts).collect();
            validate_correlated_branches(&branch_facts)?;
            for branch in all.iter() {
                check_correlation_evidence(branch, primary, scope)?;
            }
            if scope.checks_primary()
                && !branch_facts
                    .iter()
                    .any(|facts| facts.variables().contains(&primary))
            {
                return Err(QueryCompileError::MissingBinding {
                    primary_var: primary,
                });
            }
            Ok(())
        }
        QueryExprKind::Any(any) => {
            for b in any.iter() {
                check_correlation_evidence(b, primary, EvidenceScope::nested())?;
            }
            if scope.checks_primary() {
                // Every branch must contain the primary variable, but nested
                // branches are checked by their containing Any expression.
                if !any.all_branches_contain(primary) {
                    return Err(QueryCompileError::MissingBinding {
                        primary_var: primary,
                    });
                }
            }
            Ok(())
        }
        QueryExprKind::Event(eq) => {
            if scope.checks_primary() && eq.var() != primary {
                return Err(QueryCompileError::MissingBinding {
                    primary_var: primary,
                });
            }
            Ok(())
        }
        QueryExprKind::SelectEvent(s) => {
            if scope.checks_primary() && s.bind != primary {
                return Err(QueryCompileError::MissingBinding {
                    primary_var: primary,
                });
            }
            Ok(())
        }
        QueryExprKind::Require(_) | QueryExprKind::Lifecycle(_) => Ok(()),
    }
}

fn validate_correlated_branches(branches: &[QueryShapeFacts]) -> Result<(), QueryCompileError> {
    let Some(first_branch) = branches.first() else {
        return Ok(());
    };
    let first_vars: BTreeSet<VarId> = first_branch.variables().iter().copied().collect();
    let has_shared = branches.iter().skip(1).any(|branch| {
        branch
            .variables()
            .iter()
            .any(|var| first_vars.contains(var))
    });
    if has_shared || branches.len() <= 1 {
        Ok(())
    } else {
        Err(QueryCompileError::UncorrelatedConjunction)
    }
}
