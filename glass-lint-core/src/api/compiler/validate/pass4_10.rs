use std::collections::BTreeSet;

use super::{
    error::{QueryCompileError, is_identity_empty},
    pass1_3::validate_event_query,
};
use crate::api::rule::query::{
    EventSpec, IdentitySpec, LifecycleQuery, QueryDecl, QueryExpr, QueryExprKind, VarId, limits,
};

/// Correlation and scope checking (formerly Pass 5).
///
/// Rejects multi-event `All` expressions that have no compatible shared
/// variable across branches, which would produce an uncontrolled Cartesian
/// product.
#[cfg(test)]
pub(crate) fn pass_correlation_scope(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    check_correlation(decl.expression())
}

#[cfg(test)]
fn check_correlation(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::All(all) => {
            let branch_vars: Vec<Vec<VarId>> = all.branches.iter().map(QueryExpr::vars).collect();

            if branch_vars.len() > 1 {
                let first_set: BTreeSet<VarId> = branch_vars[0].iter().copied().collect();
                let has_shared = branch_vars[1..]
                    .iter()
                    .any(|vars| vars.iter().any(|v| first_set.contains(v)));
                if !has_shared {
                    return Err(QueryCompileError::UncorrelatedConjunction);
                }
            }

            for b in &all.branches {
                check_correlation(b)?;
            }
            Ok(())
        }
        QueryExprKind::Any(any) => {
            for b in &any.branches {
                check_correlation(b)?;
            }
            Ok(())
        }
        QueryExprKind::Event(_)
        | QueryExprKind::SelectEvent(_)
        | QueryExprKind::Require(_)
        | QueryExprKind::Lifecycle(_) => Ok(()),
    }
}

/// Evidence projection checking (formerly Pass 6).
///
/// Verifies that the emission's primary variable is bound on every
/// successful branch:
/// - For `Any`, every branch must contain the primary variable.
/// - For `All`, at least one branch must contain it.
/// - For `Event`, `Lifecycle`, and atomic forms, the primary variable must
///   exist.
#[cfg(test)]
pub(crate) fn pass_evidence_projection(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    let primary = decl.emission().primary_var();
    check_evidence_branch(decl.expression(), primary, true)?;
    Ok(())
}

#[cfg(test)]
fn check_evidence_branch(
    expr: &QueryExpr,
    primary: VarId,
    _is_root: bool,
) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::Any(any) => {
            for branch in &any.branches {
                if !branch.contains_var(primary) {
                    return Err(QueryCompileError::MissingBinding {
                        primary_var: primary,
                    });
                }
            }
            Ok(())
        }
        QueryExprKind::All(all) => {
            for branch in &all.branches {
                check_evidence_branch(branch, primary, false)?;
            }
            if !all.branches.iter().any(|b| b.contains_var(primary)) {
                return Err(QueryCompileError::MissingBinding {
                    primary_var: primary,
                });
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

/// Pass 7: Boundedness checking.
///
/// Rejects query shapes that cannot be bounded at compile time, such as
/// unkeyed multi-event conjunctions (handled by correlation check) and
/// unbounded collection sizes.
#[cfg(test)]
pub(crate) fn pass_boundedness(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    check_boundedness(decl.expression())
}

#[cfg(test)]
fn check_boundedness(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::Any(any) => {
            if any.branches.len() > limits::MAX_EXPR_CHILDREN {
                return Err(QueryCompileError::UnboundedQuery {
                    detail: "Any expression exceeds maximum branch count",
                });
            }
            for b in &any.branches {
                check_boundedness(b)?;
            }
            Ok(())
        }
        QueryExprKind::All(all) => {
            if all.branches.len() > limits::MAX_EXPR_CHILDREN {
                return Err(QueryCompileError::UnboundedQuery {
                    detail: "All expression exceeds maximum branch count",
                });
            }
            for b in &all.branches {
                check_boundedness(b)?;
            }
            Ok(())
        }
        QueryExprKind::Event(eq) => {
            let max_constraints = limits::MAX_PREDICATES_PER_ARGUMENT * limits::MAX_ARGUMENT_GROUPS;
            if eq.constraints().len() > max_constraints {
                return Err(QueryCompileError::UnboundedQuery {
                    detail: "Event query exceeds maximum argument constraint count",
                });
            }
            Ok(())
        }
        QueryExprKind::SelectEvent(_) | QueryExprKind::Require(_) | QueryExprKind::Lifecycle(_) => {
            Ok(())
        }
    }
}

/// Pass 8: Relation availability checking.
///
/// Validates that the required semantic relations are available for the
/// query scope.  For the current algebra, available relations are
/// determined by the event/identity/subject combination.
#[cfg(test)]
pub(crate) fn pass_relation_availability(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    check_relation_scope(decl.expression())
}

#[cfg(test)]
fn check_relation_scope(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match &expr.kind {
        QueryExprKind::Event(eq) => {
            if !identity_supports_event(eq.identity(), eq.event()) {
                return Err(QueryCompileError::UnsupportedRelation {
                    relation: eq.identity().diagnostic_name(),
                    detail: format!(
                        "identity `{}` is not available for event `{}`",
                        eq.identity().diagnostic_name(),
                        eq.event().diagnostic_name()
                    ),
                });
            }
            match eq.identity() {
                IdentitySpec::Global { name } | IdentitySpec::Heuristic { name } => {
                    if name.trim().is_empty() {
                        return Err(QueryCompileError::UnsupportedRelation {
                            relation: eq.identity().diagnostic_name(),
                            detail: "identity name is empty".into(),
                        });
                    }
                }
                IdentitySpec::Rooted { path } => {
                    if path.is_empty() {
                        return Err(QueryCompileError::UnsupportedRelation {
                            relation: "rooted",
                            detail: "rooted path is empty".into(),
                        });
                    }
                }
                IdentitySpec::ModuleExport { module, export } => {
                    if module.trim().is_empty() || export.trim().is_empty() {
                        return Err(QueryCompileError::UnsupportedRelation {
                            relation: "module_export",
                            detail: "module or export is empty".into(),
                        });
                    }
                }
                IdentitySpec::PackageModuleExport { module, export } => {
                    if module.as_str().trim().is_empty() || export.trim().is_empty() {
                        return Err(QueryCompileError::UnsupportedRelation {
                            relation: "package_module_export",
                            detail: "package pattern or export is empty".into(),
                        });
                    }
                }
                IdentitySpec::ModuleNamespace { module } => {
                    if module.trim().is_empty() {
                        return Err(QueryCompileError::UnsupportedRelation {
                            relation: "module_namespace",
                            detail: "module is empty".into(),
                        });
                    }
                }
                IdentitySpec::PackageModuleNamespace { module } => {
                    if module.as_str().trim().is_empty() {
                        return Err(QueryCompileError::UnsupportedRelation {
                            relation: "package_module_namespace",
                            detail: "package pattern is empty".into(),
                        });
                    }
                }
                IdentitySpec::LiteralString { predicate } => {
                    if predicate.trim().is_empty() {
                        return Err(QueryCompileError::UnsupportedRelation {
                            relation: "literal",
                            detail: "literal predicate is empty".into(),
                        });
                    }
                }
                IdentitySpec::PackageSpecifier { pattern } => {
                    if pattern.as_str().trim().is_empty() {
                        return Err(QueryCompileError::UnsupportedRelation {
                            relation: "package_specifier",
                            detail: "package pattern is empty".into(),
                        });
                    }
                }
            }
            Ok(())
        }
        QueryExprKind::Any(any) => {
            for b in &any.branches {
                check_relation_scope(b)?;
            }
            Ok(())
        }
        QueryExprKind::All(all) => {
            for b in &all.branches {
                check_relation_scope(b)?;
            }
            Ok(())
        }
        QueryExprKind::Lifecycle(lc) => {
            for src in lc.sources() {
                check_relation_scope(&QueryExpr::event(src.clone()))?;
            }
            Ok(())
        }
        QueryExprKind::SelectEvent(_) | QueryExprKind::Require(_) => Ok(()),
    }
}

#[cfg(test)]
fn identity_supports_event(identity: &IdentitySpec, event: &EventSpec) -> bool {
    match identity {
        IdentitySpec::Global { .. } | IdentitySpec::Heuristic { .. } => matches!(
            event,
            EventSpec::Call
                | EventSpec::Construct
                | EventSpec::ClassReference
                | EventSpec::MemberCall { .. }
                | EventSpec::MemberRead { .. }
        ),
        IdentitySpec::ModuleExport { .. } | IdentitySpec::PackageModuleExport { .. } => {
            matches!(
                event,
                EventSpec::Call | EventSpec::Construct | EventSpec::ClassReference
            )
        }
        IdentitySpec::ModuleNamespace { .. } | IdentitySpec::PackageModuleNamespace { .. } => {
            matches!(
                event,
                EventSpec::MemberCall { .. } | EventSpec::MemberRead { .. }
            )
        }
        IdentitySpec::Rooted { .. } => {
            matches!(
                event,
                EventSpec::MemberCall { .. } | EventSpec::MemberRead { .. }
            )
        }
        IdentitySpec::LiteralString { .. } => {
            matches!(event, EventSpec::Import | EventSpec::StringReference)
        }
        IdentitySpec::PackageSpecifier { .. } => matches!(event, EventSpec::Import),
    }
}

/// Pass 9: Lifecycle validation.
///
/// Validates lifecycle-specific invariants:
/// - Source must have a valid event query.
/// - Condition and completion must be consistent.
#[cfg(test)]
pub(crate) fn pass_lifecycle_validation(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    if let QueryExprKind::Lifecycle(lc) = &decl.expression().kind {
        return validate_lifecycle(lc);
    }
    Ok(())
}

fn validate_lifecycle(lc: &LifecycleQuery) -> Result<(), QueryCompileError> {
    if lc.sources().is_empty() {
        return Err(QueryCompileError::InvalidLifecycle {
            detail: "lifecycle must have at least one source".into(),
        });
    }

    for src in lc.sources() {
        validate_event_query(src)?;

        if !matches!(src.event(), EventSpec::MemberCall { .. }) {
            return Err(QueryCompileError::InvalidLifecycle {
                detail: "lifecycle source event must be a member call".into(),
            });
        }

        if !matches!(src.identity(), IdentitySpec::Rooted { .. }) {
            return Err(QueryCompileError::InvalidLifecycle {
                detail: "lifecycle source identity must be rooted".into(),
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
        QueryExprKind::SelectEvent(_) | QueryExprKind::Require(_) => Ok(()),
        QueryExprKind::Any(any) => {
            if any.branches.is_empty() {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "Any expression has zero branches (should have been rejected at construction)".into(),
                });
            }
            if any.branches.len() > limits::MAX_EXPR_CHILDREN {
                return Err(QueryCompileError::UnboundedQuery {
                    detail: "Any expression exceeds maximum branch count",
                });
            }
            for b in &any.branches {
                check_structure(b)?;
            }
            Ok(())
        }
        QueryExprKind::All(all) => {
            if all.branches.is_empty() {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "All expression has zero branches (should have been rejected at construction)".into(),
                });
            }
            if all.branches.len() > limits::MAX_EXPR_CHILDREN {
                return Err(QueryCompileError::UnboundedQuery {
                    detail: "All expression exceeds maximum branch count",
                });
            }
            for b in &all.branches {
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
            // pass_correlation_scope
            let branch_vars: Vec<Vec<VarId>> = all.branches.iter().map(QueryExpr::vars).collect();
            if branch_vars.len() > 1 {
                let first_set: BTreeSet<VarId> = branch_vars[0].iter().copied().collect();
                let has_shared = branch_vars[1..]
                    .iter()
                    .any(|vars| vars.iter().any(|v| first_set.contains(v)));
                if !has_shared {
                    return Err(QueryCompileError::UncorrelatedConjunction);
                }
            }
            // pass_evidence_projection
            for branch in &all.branches {
                check_correlation_evidence(branch, primary, false)?;
            }
            if !all.branches.iter().any(|b| b.contains_var(primary)) {
                return Err(QueryCompileError::MissingBinding {
                    primary_var: primary,
                });
            }
            Ok(())
        }
        QueryExprKind::Any(any) => {
            // pass_correlation_scope recurses into Any branches
            for b in &any.branches {
                check_correlation_scope_inner(b)?;
            }
            // pass_evidence_projection: every branch must contain the primary var
            for branch in &any.branches {
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
            let branch_vars: Vec<Vec<VarId>> = all.branches.iter().map(QueryExpr::vars).collect();
            if branch_vars.len() > 1 {
                let first_set: BTreeSet<VarId> = branch_vars[0].iter().copied().collect();
                let has_shared = branch_vars[1..]
                    .iter()
                    .any(|vars| vars.iter().any(|v| first_set.contains(v)));
                if !has_shared {
                    return Err(QueryCompileError::UncorrelatedConjunction);
                }
            }
            for b in &all.branches {
                check_correlation_scope_inner(b)?;
            }
            Ok(())
        }
        QueryExprKind::Any(any) => {
            for b in &any.branches {
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
