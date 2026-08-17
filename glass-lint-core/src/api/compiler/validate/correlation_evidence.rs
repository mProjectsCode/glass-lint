use std::collections::BTreeSet;

use super::error::QueryCompileError;
use crate::api::rule::query::{QueryDecl, QueryExpr, QueryExprKind, QueryShapeFacts, VarId};

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
        QueryExprKind::SelectEvent(bind) => {
            if scope.checks_primary() && *bind != primary {
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
