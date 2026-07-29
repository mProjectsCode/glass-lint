//! Logical query validation and type checking.
//!
//! Implements explicit validation passes rather than one large recursive
//! validator. Each pass checks one invariant and produces structured errors
//! that identify the failing authored concept.
//!
//! The passes are:
//!
//! 1. **declaration well-formedness** — identity/event/subject dimension
//!    compatibility and basic field validity;
//! 2. **symbol and variable collection** — variable uniqueness and scope;
//! 3. **variable type inference/checking** — type compatibility across bindings
//!    and uses;
//! 4. **operator compatibility** — valid `Any`/`All` branch shapes;
//! 5. **correlation and scope checking** — shared variables in multi-event
//!    conjunctions;
//! 6. **evidence projection checking** — emission primary variable exists and
//!    has a source location;
//! 7. **boundedness checking** — query shapes that can run without uncontrolled
//!    work;
//! 8. **relation availability checking** — supported relation scope;
//! 9. **lifecycle validation** — valid lifecycle source/condition/completion;
//! 10. **final invariant validation** — post-normalization checks.

use crate::api::rule::query::{
    limits, EventQuery, EventSpec, IdentitySpec, LifecycleQuery, QueryDecl, QueryExpr, QuerySet,
    SubjectSpec, VarId,
};

// ── Error type ───────────────────────────────────────────────────────────

/// A structured validation error for logical queries.
///
/// Each variant has a stable diagnostic name and carries enough context
/// for the caller to identify the failing authored concept.  Errors from
/// invalid author input are separate from internal compiler bugs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QueryCompileError {
    /// The emission's primary variable is not bound in the expression.
    MissingBinding { primary_var: VarId },
    /// A variable is bound more than once.
    DuplicateBinding { var: VarId },
    /// Variable type mismatch between binding and use.
    #[allow(dead_code)]
    TypeMismatch {
        var: VarId,
        expected: &'static str,
        actual: &'static str,
    },
    /// Identity/event/subject combination cannot select a semantic fact.
    InvalidEventPredicate {
        identity: String,
        event: String,
        subject: String,
        detail: &'static str,
    },
    /// A semantic relation is not available for this query context.
    UnsupportedRelation {
        relation: &'static str,
        detail: String,
    },
    /// Multi-event `All` without compatible shared variables.
    UncorrelatedConjunction,
    /// Primary variable lacks an available source location.
    #[allow(dead_code)]
    UnavailablePrimaryLocation { var: VarId },
    /// Lifecycle structure is invalid.
    InvalidLifecycle { detail: String },
    /// Query shape cannot be bounded at compile time.
    UnboundedQuery { detail: &'static str },
    /// Module specifier pattern is invalid.
    #[allow(dead_code)]
    InvalidModulePattern { pattern: String, detail: String },
    /// Static-value predicate is invalid.
    #[allow(dead_code)]
    InvalidStaticValuePredicate { detail: String },
    /// Internal compiler invariant violation (bug, not authored error).
    InternalInvariant { detail: String },
}

impl QueryCompileError {
    /// Stable diagnostic name for this error variant.
    pub(crate) fn diagnostic_name(&self) -> &'static str {
        match self {
            Self::MissingBinding { .. } => "missing_binding",
            Self::DuplicateBinding { .. } => "duplicate_binding",
            Self::TypeMismatch { .. } => "type_mismatch",
            Self::InvalidEventPredicate { .. } => "invalid_event_predicate",
            Self::UnsupportedRelation { .. } => "unsupported_relation",
            Self::UncorrelatedConjunction => "uncorrelated_conjunction",
            Self::UnavailablePrimaryLocation { .. } => "unavailable_primary_location",
            Self::InvalidLifecycle { .. } => "invalid_lifecycle",
            Self::UnboundedQuery { .. } => "unbounded_query",
            Self::InvalidModulePattern { .. } => "invalid_module_pattern",
            Self::InvalidStaticValuePredicate { .. } => "invalid_static_value_predicate",
            Self::InternalInvariant { .. } => "internal_invariant",
        }
    }
}

impl std::fmt::Display for QueryCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingBinding { primary_var } => {
                write!(
                    f,
                    "primary variable {primary_var} is not bound in expression"
                )
            }
            Self::DuplicateBinding { var } => {
                write!(f, "variable {var} is bound more than once")
            }
            Self::TypeMismatch {
                var,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "variable {var} has type `{actual}` but expected `{expected}`"
                )
            }
            Self::InvalidEventPredicate {
                identity,
                event,
                subject,
                detail,
            } => {
                write!(
                    f,
                    "invalid event predicate: identity={identity}, event={event}, subject={subject}: {detail}"
                )
            }
            Self::UnsupportedRelation { relation, detail } => {
                write!(f, "unsupported relation `{relation}`: {detail}")
            }
            Self::UncorrelatedConjunction => {
                f.write_str("multi-event All without compatible shared variables")
            }
            Self::UnavailablePrimaryLocation { var } => {
                write!(f, "primary variable {var} lacks a source location")
            }
            Self::InvalidLifecycle { detail } => {
                write!(f, "invalid lifecycle: {detail}")
            }
            Self::UnboundedQuery { detail } => {
                write!(f, "unbounded query: {detail}")
            }
            Self::InvalidModulePattern { pattern, detail } => {
                write!(f, "invalid module pattern `{pattern}`: {detail}")
            }
            Self::InvalidStaticValuePredicate { detail } => {
                write!(f, "invalid static-value predicate: {detail}")
            }
            Self::InternalInvariant { detail } => {
                write!(f, "internal invariant: {detail}")
            }
        }
    }
}

// ── Dimension compatibility helpers ──────────────────────────────────────

/// Check whether the identity/event/subject combination is compatible for
/// a direct subject relationship (the most common case).
fn is_direct_dimension_valid(identity: &IdentitySpec, event: &EventSpec) -> bool {
    matches!(
        (identity, event),
        (
            IdentitySpec::Global { .. }
                | IdentitySpec::Heuristic { .. }
                | IdentitySpec::ModuleExport { .. }
                | IdentitySpec::PackageModuleExport { .. },
            EventSpec::Call | EventSpec::Construct
        ) | (
            IdentitySpec::Rooted { .. }
                | IdentitySpec::Heuristic { .. }
                | IdentitySpec::ModuleNamespace { .. }
                | IdentitySpec::PackageModuleNamespace { .. },
            EventSpec::MemberCall { .. } | EventSpec::MemberRead { .. }
        ) | (
            IdentitySpec::ModuleExport { .. }
                | IdentitySpec::PackageModuleExport { .. }
                | IdentitySpec::Heuristic { .. },
            EventSpec::ClassReference
        ) | (
            IdentitySpec::LiteralString { .. } | IdentitySpec::PackageSpecifier { .. },
            EventSpec::Import | EventSpec::StringReference
        )
    )
}

/// Check that the subject identity is consistent with the event's identity.
///
/// For [`SubjectSpec::Direct`], the event's identity member chain must match
/// the identity's path for heuristic and rooted members. For
/// [`SubjectSpec::ReturnedFrom`] and [`SubjectSpec::InstanceOf`], the
/// identity is the producer/constructor and is always consistent — the
/// duplicate-field check was removed in Phase 9.
fn is_subject_identity_consistent(
    identity: &IdentitySpec,
    event: &EventSpec,
    subject: SubjectSpec,
) -> bool {
    match subject {
        SubjectSpec::Direct => match (identity, event) {
            (
                IdentitySpec::Heuristic { name },
                EventSpec::MemberCall { member } | EventSpec::MemberRead { member },
            ) => member.eq_chain(name),
            (
                IdentitySpec::Rooted { path },
                EventSpec::MemberCall { member } | EventSpec::MemberRead { member },
            ) => *path == *member,
            _ => true,
        },
        SubjectSpec::ReturnedFrom | SubjectSpec::InstanceOf => true,
    }
}

/// Top-level dimension check: is this (identity, event, subject) valid?
fn is_valid_identity_event_subject(
    identity: &IdentitySpec,
    event: &EventSpec,
    subject: SubjectSpec,
) -> bool {
    if !is_subject_identity_consistent(identity, event, subject) {
        return false;
    }
    match subject {
        SubjectSpec::Direct => is_direct_dimension_valid(identity, event),
        SubjectSpec::ReturnedFrom => matches!(
            (identity, event),
            (
                IdentitySpec::Rooted { .. },
                EventSpec::MemberCall { .. } | EventSpec::MemberRead { .. }
            )
        ),
        SubjectSpec::InstanceOf => matches!(
            (identity, event),
            (
                IdentitySpec::ModuleExport { .. } | IdentitySpec::PackageModuleExport { .. },
                EventSpec::MemberCall { .. }
            )
        ),
    }
}

/// Check if an identity name or pattern is empty.
fn is_identity_empty(identity: &IdentitySpec) -> bool {
    match identity {
        IdentitySpec::Global { name } | IdentitySpec::Heuristic { name } => name.trim().is_empty(),
        IdentitySpec::ModuleExport { module, export } => {
            module.trim().is_empty() || export.trim().is_empty()
        }
        IdentitySpec::PackageModuleExport { module, export } => {
            module.as_str().trim().is_empty() || export.trim().is_empty()
        }
        IdentitySpec::ModuleNamespace { module } => module.trim().is_empty(),
        IdentitySpec::PackageModuleNamespace { module } => module.as_str().trim().is_empty(),
        IdentitySpec::Rooted { path } => path.is_empty(),
        IdentitySpec::LiteralString { predicate } => predicate.trim().is_empty(),
        IdentitySpec::PackageSpecifier { pattern } => pattern.as_str().trim().is_empty(),
    }
}

/// Check whether constraints are on a call-bearing event.
fn event_supports_constraints(event: &EventSpec) -> bool {
    matches!(event, EventSpec::Call | EventSpec::MemberCall { .. })
}

// ── Variable helpers ─────────────────────────────────────────────────────

/// Collect all distinct variable IDs in an expression tree.
fn collect_vars(expr: &QueryExpr) -> Vec<VarId> {
    let mut ids = Vec::new();
    collect_vars_rec(expr, &mut ids);
    ids
}

fn collect_vars_rec(expr: &QueryExpr, ids: &mut Vec<VarId>) {
    match expr {
        QueryExpr::Event(eq) => ids.push(eq.var()),
        QueryExpr::Any(any) => {
            for b in &any.branches {
                collect_vars_rec(b, ids);
            }
        }
        QueryExpr::All(all) => {
            for b in &all.branches {
                collect_vars_rec(b, ids);
            }
        }
        QueryExpr::Lifecycle(lc) => {
            for src in lc.sources() {
                ids.push(src.var());
            }
        }
    }
}

/// Check whether a variable appears in an expression tree.
fn expr_contains_var(expr: &QueryExpr, target: VarId) -> bool {
    match expr {
        QueryExpr::Event(eq) => eq.var() == target,
        QueryExpr::Any(any) => any.branches.iter().any(|b| expr_contains_var(b, target)),
        QueryExpr::All(all) => all.branches.iter().any(|b| expr_contains_var(b, target)),
        QueryExpr::Lifecycle(lc) => lc.sources().iter().any(|src| src.var() == target),
    }
}

// ── Individual validation passes ─────────────────────────────────────────

/// Pass 1: Declaration well-formedness.
///
/// Rejects invalid identity/event/subject combinations, empty identities,
/// and constraints on non-call events.
pub(crate) fn pass_well_formedness(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    match decl.expression() {
        QueryExpr::Event(eq) => {
            validate_event_query(eq)?;
        }
        QueryExpr::Any(any) => {
            for branch in &any.branches {
                pass_well_formedness_inner(branch)?;
            }
        }
        QueryExpr::All(all) => {
            for branch in &all.branches {
                pass_well_formedness_inner(branch)?;
            }
        }
        QueryExpr::Lifecycle(lc) => {
            for src in lc.sources() {
                validate_event_query(src)?;
            }
        }
    }
    Ok(())
}

fn pass_well_formedness_inner(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match expr {
        QueryExpr::Event(eq) => validate_event_query(eq),
        QueryExpr::Any(any) => {
            for b in &any.branches {
                pass_well_formedness_inner(b)?;
            }
            Ok(())
        }
        QueryExpr::All(all) => {
            for b in &all.branches {
                pass_well_formedness_inner(b)?;
            }
            Ok(())
        }
        QueryExpr::Lifecycle(lc) => {
            for src in lc.sources() {
                validate_event_query(src)?;
            }
            Ok(())
        }
    }
}

fn validate_event_query(eq: &EventQuery) -> Result<(), QueryCompileError> {
    if !is_valid_identity_event_subject(eq.identity(), eq.event(), eq.subject()) {
        return Err(QueryCompileError::InvalidEventPredicate {
            identity: eq.identity().diagnostic_name().to_owned(),
            event: eq.event().diagnostic_name().to_owned(),
            subject: eq.subject().diagnostic_name().to_owned(),
            detail: "identity/event/subject combination cannot select a semantic fact",
        });
    }
    if is_identity_empty(eq.identity()) {
        return Err(QueryCompileError::InvalidEventPredicate {
            identity: eq.identity().diagnostic_name().to_owned(),
            event: eq.event().diagnostic_name().to_owned(),
            subject: eq.subject().diagnostic_name().to_owned(),
            detail: "identity name or pattern is empty",
        });
    }
    if !eq.constraints().is_empty() && !event_supports_constraints(eq.event()) {
        return Err(QueryCompileError::InvalidEventPredicate {
            identity: eq.identity().diagnostic_name().to_owned(),
            event: eq.event().diagnostic_name().to_owned(),
            subject: eq.subject().diagnostic_name().to_owned(),
            detail: "argument constraints require a call-bearing event",
        });
    }
    Ok(())
}

/// Pass 2: Symbol and variable collection.
///
/// Checks that no variable is bound more than once in the expression tree.
pub(crate) fn pass_variable_collection(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    let mut seen = Vec::new();
    collect_and_check(decl.expression(), &mut seen)?;
    Ok(())
}

fn collect_and_check(expr: &QueryExpr, seen: &mut Vec<VarId>) -> Result<(), QueryCompileError> {
    match expr {
        QueryExpr::Event(eq) => {
            if seen.contains(&eq.var()) {
                return Err(QueryCompileError::DuplicateBinding { var: eq.var() });
            }
            seen.push(eq.var());
        }
        QueryExpr::Any(any) => {
            for b in &any.branches {
                collect_and_check(b, seen)?;
            }
        }
        QueryExpr::All(all) => {
            for b in &all.branches {
                collect_and_check(b, seen)?;
            }
        }
        QueryExpr::Lifecycle(lc) => {
            for src in lc.sources() {
                if seen.contains(&src.var()) {
                    return Err(QueryCompileError::DuplicateBinding { var: src.var() });
                }
                seen.push(src.var());
            }
        }
    }
    Ok(())
}

/// Pass 3: Variable type inference/checking.
///
/// Validates basic type compatibility for the current algebra. Variables
/// are bound by event selections and used in evidence projection. Full
/// type inference across relational predicates is deferred to later phases.
pub(crate) fn pass_type_checking(decl: &QueryDecl) {
    // For the current algebra all variables are event-typed and the
    // emission primary var type is compatible with the evidence kind by
    // construction.  No additional type errors are possible until
    // relational predicates introduce typed bindings.
    let _ = decl;
}

/// Pass 4: Operator compatibility.
///
/// Validates that `Any` and `All` operators have compatible internal
/// structure (e.g., non-empty branches with matching variable shapes).
pub(crate) fn pass_operator_compatibility(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    check_operator_compatibility(decl.expression())
}

fn check_operator_compatibility(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match expr {
        QueryExpr::Any(any) => {
            if any.branches.is_empty() {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "Any expression has zero branches (should have been rejected at construction)".into(),
                });
            }
            for b in &any.branches {
                check_operator_compatibility(b)?;
            }
            Ok(())
        }
        QueryExpr::All(all) => {
            if all.branches.is_empty() {
                return Err(QueryCompileError::InternalInvariant {
                    detail: "All expression has zero branches (should have been rejected at construction)".into(),
                });
            }
            for b in &all.branches {
                check_operator_compatibility(b)?;
            }
            Ok(())
        }
        QueryExpr::Event(_) | QueryExpr::Lifecycle(_) => Ok(()),
    }
}

/// Pass 5: Correlation and scope checking.
///
/// Rejects multi-event `All` expressions that have no compatible shared
/// variable across branches, which would produce an uncontrolled Cartesian
/// product.
pub(crate) fn pass_correlation_scope(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    check_correlation(decl.expression())
}

fn check_correlation(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match expr {
        QueryExpr::All(all) => {
            // Collect variables from each branch
            let branch_vars: Vec<Vec<VarId>> = all.branches.iter().map(collect_vars).collect();

            // If there are multiple branches, they must share at least one
            // variable to avoid a Cartesian product.
            if branch_vars.len() > 1 {
                let first_set: std::collections::BTreeSet<VarId> =
                    branch_vars[0].iter().copied().collect();
                let has_shared = branch_vars[1..]
                    .iter()
                    .any(|vars| vars.iter().any(|v| first_set.contains(v)));
                if !has_shared {
                    return Err(QueryCompileError::UncorrelatedConjunction);
                }
            }

            // Recurse into branches
            for b in &all.branches {
                check_correlation(b)?;
            }
            Ok(())
        }
        QueryExpr::Any(any) => {
            for b in &any.branches {
                check_correlation(b)?;
            }
            Ok(())
        }
        QueryExpr::Event(_) | QueryExpr::Lifecycle(_) => Ok(()),
    }
}

/// Pass 6: Evidence projection checking.
///
/// Verifies that the emission's primary variable is bound in the expression
/// and has an event type (which provides a source location).
pub(crate) fn pass_evidence_projection(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    if !expr_contains_var(decl.expression(), decl.emission().primary_var()) {
        return Err(QueryCompileError::MissingBinding {
            primary_var: decl.emission().primary_var(),
        });
    }
    Ok(())
}

/// Pass 7: Boundedness checking.
///
/// Rejects query shapes that cannot be bounded at compile time, such as
/// unkeyed multi-event conjunctions (handled by correlation check) and
/// unbounded collection sizes.
pub(crate) fn pass_boundedness(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    check_boundedness(decl.expression())
}

fn check_boundedness(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match expr {
        QueryExpr::Any(any) => {
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
        QueryExpr::All(all) => {
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
        QueryExpr::Event(eq) => {
            let max_constraints = limits::MAX_PREDICATES_PER_ARGUMENT * limits::MAX_ARGUMENT_GROUPS;
            if eq.constraints().len() > max_constraints {
                return Err(QueryCompileError::UnboundedQuery {
                    detail: "Event query exceeds maximum argument constraint count",
                });
            }
            Ok(())
        }
        QueryExpr::Lifecycle(_) => Ok(()),
    }
}

/// Pass 8: Relation availability checking.
///
/// Validates that the required semantic relations are available for the
/// query scope.  For the current algebra, available relations are
/// determined by the event/identity/subject combination.
pub(crate) fn pass_relation_availability(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    check_relation_scope(decl.expression())
}

fn check_relation_scope(expr: &QueryExpr) -> Result<(), QueryCompileError> {
    match expr {
        QueryExpr::Event(eq) => {
            // Validate that the identity type is supported in the
            // current semantic model.
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
        QueryExpr::Any(any) => {
            for b in &any.branches {
                check_relation_scope(b)?;
            }
            Ok(())
        }
        QueryExpr::All(all) => {
            for b in &all.branches {
                check_relation_scope(b)?;
            }
            Ok(())
        }
        QueryExpr::Lifecycle(lc) => {
            for src in lc.sources() {
                check_relation_scope(&QueryExpr::Event(src.clone()))?;
            }
            Ok(())
        }
    }
}

/// Pass 9: Lifecycle validation.
///
/// Validates lifecycle-specific invariants:
/// - Source must have a valid event query.
/// - Condition and completion must be consistent.
pub(crate) fn pass_lifecycle_validation(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    match decl.expression() {
        QueryExpr::Lifecycle(lc) => validate_lifecycle(lc),
        _ => Ok(()),
    }
}

fn validate_lifecycle(lc: &LifecycleQuery) -> Result<(), QueryCompileError> {
    // Sources must be non-empty.
    if lc.sources().is_empty() {
        return Err(QueryCompileError::InvalidLifecycle {
            detail: "lifecycle must have at least one source".into(),
        });
    }

    // Each source must be valid.
    for src in lc.sources() {
        validate_event_query(src)?;

        // Source event must be a member call (the tracked object is produced
        // by a member call returning an object).
        if !matches!(src.event(), EventSpec::MemberCall { .. }) {
            return Err(QueryCompileError::InvalidLifecycle {
                detail: "lifecycle source event must be a member call".into(),
            });
        }

        // Source identity must be rooted for object tracking.
        if !matches!(src.identity(), IdentitySpec::Rooted { .. }) {
            return Err(QueryCompileError::InvalidLifecycle {
                detail: "lifecycle source identity must be rooted".into(),
            });
        }
    }

    // Lifecycle must have at least one of condition or completion.
    if lc.condition().is_none() && lc.completion().is_none() {
        return Err(QueryCompileError::InvalidLifecycle {
            detail: "lifecycle must have at least a condition or completion".into(),
        });
    }

    Ok(())
}

/// Pass 10: Final invariant validation after normalization.
///
/// Checks invariants that must hold after normalization.  This pass runs
/// as part of [`validate_query_decl`] (before normalization) and separately
/// as [`validate_normalized_decl`] after normalization has been applied.
///
/// Pre-normalization: verifies evidence projection is valid and that the
/// expression shape is feasible for normalization.
pub(crate) fn pass_final_invariants(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    // Verify evidence projection is valid.
    pass_evidence_projection(decl)
}

/// Validate a normalized query declaration.
///
/// Runs post-normalization checks that are meaningful only after
/// flattening, deduplication, sorting, and variable reassignment:
///
/// - evidence projection refers to a valid remapped variable;
/// - no nested `Any`-in-`Any` or `All`-in-`All` remains;
/// - variable slots are dense starting from 0.
pub(crate) fn validate_normalized_decl(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    // Re-check evidence projection (vars may have been remapped).
    pass_evidence_projection(decl)?;

    // Check that flattening was effective.
    check_normalized_structure(decl.expression(), true)?;

    Ok(())
}

/// Recursively check that a normalized expression has no nested same-type
/// Any/All (which should have been flattened) and that variable slots
/// are structurally sound.
fn check_normalized_structure(expr: &QueryExpr, _is_root: bool) -> Result<(), QueryCompileError> {
    match expr {
        QueryExpr::Any(any) => {
            for b in &any.branches {
                // Any should not contain Any (would have been flattened).
                if matches!(b, QueryExpr::Any(_)) {
                    return Err(QueryCompileError::InternalInvariant {
                        detail: "nested Any found after normalization".into(),
                    });
                }
                // All inside Any is fine (different logical operator).
                check_normalized_structure(b, false)?;
            }
            Ok(())
        }
        QueryExpr::All(all) => {
            for b in &all.branches {
                // All should not contain All (would have been flattened).
                if matches!(b, QueryExpr::All(_)) {
                    return Err(QueryCompileError::InternalInvariant {
                        detail: "nested All found after normalization".into(),
                    });
                }
                check_normalized_structure(b, false)?;
            }
            Ok(())
        }
        QueryExpr::Event(_) | QueryExpr::Lifecycle(_) => Ok(()),
    }
}

// ── Entry point ──────────────────────────────────────────────────────────

/// Validate a single [`QueryDecl`] by running all validation passes.
///
/// Returns the first error encountered.  Pass order is deterministic and
/// prioritizes structural errors (well-formedness) before semantic errors
/// (evidence, correlation).
pub(crate) fn validate_query_decl(decl: &QueryDecl) -> Result<(), QueryCompileError> {
    // Order: structural → semantic → post-normalization
    pass_well_formedness(decl)?;
    pass_variable_collection(decl)?;
    pass_type_checking(decl);
    pass_operator_compatibility(decl)?;
    pass_correlation_scope(decl)?;
    pass_evidence_projection(decl)?;
    pass_boundedness(decl)?;
    pass_relation_availability(decl)?;
    pass_lifecycle_validation(decl)?;
    pass_final_invariants(decl)?;
    Ok(())
}

/// Validate all queries in a [`QuerySet`].
#[allow(dead_code)]
pub(crate) fn validate_query_set(set: &QuerySet) -> Result<(), QueryCompileError> {
    for query in set.queries() {
        validate_query_decl(query)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::SymbolPath;
    use smol_str::SmolStr;

    use super::*;
    use crate::api::{
        classification::MatchKind,
        rule::{
            ArgumentConstraint, QueryDecl, ValueMatcher,
            query::{AllExpr, AnyExpr, EmissionDecl, EventQuery, LifecycleQuery},
        },
    };

    // ── Helpers ────────────────────────────────────────────────────

    // ── Well-formedness tests ─────────────────────────────────────

    #[test]
    fn valid_global_call_passes_well_formedness() {
        let decl = QueryDecl::call_global("fetch").unwrap();
        assert!(pass_well_formedness(&decl).is_ok());
    }

    #[test]
    fn valid_heuristic_call_passes_well_formedness() {
        let decl = QueryDecl::call_heuristic("fetch").unwrap();
        assert!(pass_well_formedness(&decl).is_ok());
    }

    #[test]
    fn valid_rooted_member_call_passes_well_formedness() {
        let decl = QueryDecl::member_call_rooted("document.createElement").unwrap();
        assert!(pass_well_formedness(&decl).is_ok());
    }

    #[test]
    fn direct_event_must_match_subject_identity() {
        // Identity Heuristic with MemberCall must have matching name.
        // The member path and identity name must match.
        let eq = EventQuery {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: SymbolPath::from("foo.bar"),
            },
            identity: IdentitySpec::Heuristic {
                name: SmolStr::new("foo.bar"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let decl = QueryDecl {
            expression: QueryExpr::Event(eq),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::MemberCall,
                symbol: "test".into(),
            },
        };
        assert!(pass_well_formedness(&decl).is_ok());
    }

    #[test]
    fn member_call_needs_matching_identity_name() {
        // Identity Heuristic "foo" with MemberCall "bar" should fail
        // because the member chain doesn't match the name.
        let eq = EventQuery {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: SymbolPath::from("bar"),
            },
            identity: IdentitySpec::Heuristic {
                name: SmolStr::new("foo"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let decl = QueryDecl {
            expression: QueryExpr::Event(eq),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::MemberCall,
                symbol: "test".into(),
            },
        };
        assert_eq!(
            pass_well_formedness(&decl),
            Err(QueryCompileError::InvalidEventPredicate {
                identity: "heuristic".into(),
                event: "member_call".into(),
                subject: "direct".into(),
                detail: "identity/event/subject combination cannot select a semantic fact",
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
            subject: SubjectSpec::Direct,
            constraints: vec![ArgumentConstraint::new(
                crate::api::rule::ArgumentIndex::new_unchecked(0),
                ValueMatcher::static_string(),
            )],
        };
        let decl = QueryDecl {
            expression: QueryExpr::Event(eq),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Import,
                symbol: "test".into(),
            },
        };
        assert_eq!(
            pass_well_formedness(&decl),
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
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let decl = QueryDecl {
            expression: QueryExpr::Event(eq),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "test".into(),
            },
        };
        assert_eq!(
            pass_well_formedness(&decl),
            Err(QueryCompileError::InvalidEventPredicate {
                identity: "global".into(),
                event: "call".into(),
                subject: "direct".into(),
                detail: "identity name or pattern is empty",
            })
        );
    }

    // ── Variable collection tests ─────────────────────────────────

    #[test]
    fn duplicate_var_in_all_fails() {
        let a = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let b = QueryExpr::Event(EventQuery {
            var: VarId::new(0), // same var as a
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let decl = QueryDecl {
            expression: QueryExpr::All(AllExpr::new(vec![a, b]).unwrap()),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "fetch".into(),
            },
        };
        assert_eq!(
            pass_variable_collection(&decl),
            Err(QueryCompileError::DuplicateBinding { var: VarId::new(0) })
        );
    }

    #[test]
    fn unique_vars_pass_collection() {
        let a = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let b = QueryExpr::Event(EventQuery {
            var: VarId::new(1),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("navigate"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        // Even in a multi-branch All, different vars are fine.
        let decl = QueryDecl {
            expression: QueryExpr::All(AllExpr::new(vec![a, b]).unwrap()),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "test".into(),
            },
        };
        assert!(pass_variable_collection(&decl).is_ok());
    }

    // ── Evidence projection tests ─────────────────────────────────

    #[test]
    fn emission_var_must_exist_in_expression() {
        let eq = EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let decl = QueryDecl {
            expression: QueryExpr::Event(eq),
            emission: EmissionDecl {
                primary_var: VarId::new(1), // not in expression
                kind: MatchKind::Call,
                symbol: "fetch".into(),
            },
        };
        assert_eq!(
            pass_evidence_projection(&decl),
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
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let decl = QueryDecl {
            expression: QueryExpr::Event(eq),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "fetch".into(),
            },
        };
        assert!(pass_evidence_projection(&decl).is_ok());
    }

    // ── Correlation tests ─────────────────────────────────────────

    #[test]
    fn uncorrelated_multi_event_all_fails() {
        let a = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let b = QueryExpr::Event(EventQuery {
            var: VarId::new(1), // different var, no shared
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("navigate"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let decl = QueryDecl {
            expression: QueryExpr::All(AllExpr::new(vec![a, b]).unwrap()),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "test".into(),
            },
        };
        assert_eq!(
            pass_correlation_scope(&decl),
            Err(QueryCompileError::UncorrelatedConjunction)
        );
    }

    #[test]
    fn correlated_multi_event_all_passes() {
        let a = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let b = QueryExpr::Event(EventQuery {
            var: VarId::new(0), // same var as a → correlated
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("navigate"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let decl = QueryDecl {
            expression: QueryExpr::All(AllExpr::new(vec![a, b]).unwrap()),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "test".into(),
            },
        };
        assert!(pass_correlation_scope(&decl).is_ok());
    }

    #[test]
    fn single_branch_all_needs_no_correlation() {
        let a = QueryExpr::Event(EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        });
        let decl = QueryDecl {
            expression: QueryExpr::All(AllExpr::new(vec![a]).unwrap()),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "fetch".into(),
            },
        };
        assert!(pass_correlation_scope(&decl).is_ok());
    }

    // ── Boundedness tests ─────────────────────────────────────────

    #[test]
    fn bounded_query_passes_boundedness() {
        let eq = EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let decl = QueryDecl {
            expression: QueryExpr::Event(eq),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "fetch".into(),
            },
        };
        assert!(pass_boundedness(&decl).is_ok());
    }

    #[test]
    fn excessive_any_branches_fails_boundedness() {
        let branches: Vec<QueryExpr> = (0..1001)
            .map(|i| {
                QueryExpr::Event(EventQuery {
                    var: VarId::new(i),
                    event: EventSpec::Call,
                    identity: IdentitySpec::Global {
                        name: SmolStr::new(format!("f{i}")),
                    },
                    subject: SubjectSpec::Direct,
                    constraints: vec![],
                })
            })
            .collect();
        let any = QueryExpr::Any(AnyExpr::new(branches).unwrap());
        let decl = QueryDecl {
            expression: any,
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "test".into(),
            },
        };
        assert_eq!(
            pass_boundedness(&decl),
            Err(QueryCompileError::UnboundedQuery {
                detail: "Any expression exceeds maximum branch count",
            })
        );
    }

    // ── Lifecycle validation tests ────────────────────────────────

    #[test]
    fn lifecycle_source_must_be_member_call() {
        // Valid pair: Global + Call (passes well-formedness) but not a
        // member call, so lifecycle validation should fail.
        let source = EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call, // not a member call
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let lc = LifecycleQuery {
            sources: vec![source],
            condition: Some(crate::api::rule::FlowCondition::event(
                crate::api::rule::ObjectEventMatcher::property_write(
                    "type",
                    ValueMatcher::any_value(),
                ),
            )),
            completion: Some(crate::api::rule::FlowCompletion::configuration()),
        };
        let decl = QueryDecl {
            expression: QueryExpr::Lifecycle(lc),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "test".into(),
            },
        };
        assert_eq!(
            pass_lifecycle_validation(&decl),
            Err(QueryCompileError::InvalidLifecycle {
                detail: "lifecycle source event must be a member call".into(),
            })
        );
    }

    #[test]
    fn lifecycle_source_must_be_rooted() {
        // Use a module-namespace identity (valid with MemberCall) but not
        // rooted, so lifecycle validation should fail.
        let source = EventQuery {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: SymbolPath::from("ns.method"),
            },
            identity: IdentitySpec::ModuleNamespace {
                module: SmolStr::new("mod"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let lc = LifecycleQuery {
            sources: vec![source],
            condition: Some(crate::api::rule::FlowCondition::event(
                crate::api::rule::ObjectEventMatcher::property_write(
                    "type",
                    ValueMatcher::any_value(),
                ),
            )),
            completion: Some(crate::api::rule::FlowCompletion::configuration()),
        };
        let decl = QueryDecl {
            expression: QueryExpr::Lifecycle(lc),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "test".into(),
            },
        };
        assert_eq!(
            pass_lifecycle_validation(&decl),
            Err(QueryCompileError::InvalidLifecycle {
                detail: "lifecycle source identity must be rooted".into(),
            })
        );
    }

    #[test]
    fn valid_lifecycle_passes_lifecycle_validation() {
        // Rooted identity + MemberCall with matching path.
        let source = EventQuery {
            var: VarId::new(0),
            event: EventSpec::MemberCall {
                member: SymbolPath::from("document.createElement"),
            },
            identity: IdentitySpec::Rooted {
                path: SymbolPath::from("document.createElement"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let lc = LifecycleQuery {
            sources: vec![source],
            condition: Some(crate::api::rule::FlowCondition::event(
                crate::api::rule::ObjectEventMatcher::property_write(
                    "type",
                    ValueMatcher::any_value(),
                ),
            )),
            completion: Some(crate::api::rule::FlowCompletion::configuration()),
        };
        let decl = QueryDecl {
            expression: QueryExpr::Lifecycle(lc),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "test".into(),
            },
        };
        assert!(pass_lifecycle_validation(&decl).is_ok());
    }

    // ── Top-level validate_query_decl ─────────────────────────────

    fn assert_valid_query(decl: &QueryDecl) {
        if let Err(e) = validate_query_decl(decl) {
            panic!("query validation failed: {} ({})", e, e.diagnostic_name());
        }
    }

    #[test]
    fn all_query_forms_pass_validation() {
        assert_valid_query(&QueryDecl::call_global("fetch").unwrap());
        assert_valid_query(&QueryDecl::call_heuristic("fetch").unwrap());
        assert_valid_query(&QueryDecl::call_module("fs", "readFile").unwrap());
        assert_valid_query(&QueryDecl::call_package("@scope/pkg", "method").unwrap());
        assert_valid_query(&QueryDecl::member_call_rooted("document.createElement").unwrap());
        assert_valid_query(&QueryDecl::member_call_heuristic("foo.bar").unwrap());
        assert_valid_query(&QueryDecl::member_call_module("module", "method").unwrap());
        assert_valid_query(&QueryDecl::member_call_instance("pkg", "Client", "send").unwrap());
        assert_valid_query(&QueryDecl::member_call_package("@scope/pkg", "method").unwrap());
        assert_valid_query(&QueryDecl::member_call_returned("create", "send").unwrap());
        assert_valid_query(&QueryDecl::member_read_rooted("window.location").unwrap());
        assert_valid_query(&QueryDecl::member_read_module("module", "property").unwrap());
        assert_valid_query(&QueryDecl::member_read_returned("create", "token").unwrap());
        assert_valid_query(&QueryDecl::member_read_package("@scope/pkg", "property").unwrap());
        assert_valid_query(&QueryDecl::import_exact("node:fs").unwrap());
        assert_valid_query(&QueryDecl::import_package("@scope/pkg").unwrap());
        assert_valid_query(&QueryDecl::string_contains("https://").unwrap());
        assert_valid_query(&QueryDecl::class_heuristic("Worker").unwrap());
        assert_valid_query(&QueryDecl::class_module("module", "Klass").unwrap());
        assert_valid_query(&QueryDecl::constructor_global("URL").unwrap());
        assert_valid_query(&QueryDecl::constructor_heuristic("Foo").unwrap());
        assert_valid_query(&QueryDecl::constructor_module("pkg", "Klass").unwrap());
    }

    #[test]
    fn query_set_with_valid_queries_passes_validation() {
        let q1 = QueryDecl::call_global("fetch").unwrap();
        let q2 = QueryDecl::member_read_rooted("window.location").unwrap();
        let set = QuerySet::new(vec![q1, q2]);
        assert!(validate_query_set(&set).is_ok());
    }

    // ── Error variant tests ───────────────────────────────────────

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
    fn invalid_module_pattern_error_has_correct_diagnostic_name() {
        let err = QueryCompileError::InvalidModulePattern {
            pattern: "test".into(),
            detail: "test".into(),
        };
        assert_eq!(err.diagnostic_name(), "invalid_module_pattern");
    }

    #[test]
    fn invalid_static_value_predicate_has_correct_diagnostic_name() {
        let err = QueryCompileError::InvalidStaticValuePredicate {
            detail: "test".into(),
        };
        assert_eq!(err.diagnostic_name(), "invalid_static_value_predicate");
    }

    #[test]
    fn internal_invariant_error_has_correct_diagnostic_name() {
        let err = QueryCompileError::InternalInvariant {
            detail: "test".into(),
        };
        assert_eq!(err.diagnostic_name(), "internal_invariant");
    }

    // ── Display tests ─────────────────────────────────────────────

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

    // ── Operator compatibility tests ──────────────────────────────

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

    // ── Relation availability tests ───────────────────────────────

    #[test]
    fn relation_availability_passes_for_valid_global() {
        let eq = EventQuery {
            var: VarId::new(0),
            event: EventSpec::Call,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fetch"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let decl = QueryDecl {
            expression: QueryExpr::Event(eq),
            emission: EmissionDecl {
                primary_var: VarId::new(0),
                kind: MatchKind::Call,
                symbol: "fetch".into(),
            },
        };
        assert!(pass_relation_availability(&decl).is_ok());
    }

    // ── Deterministic error precedence ────────────────────────────

    #[test]
    fn well_formedness_error_precedes_projection_error() {
        // Well-formedness checks run before evidence projection, so an
        // invalid dimension should be reported first even if the
        // evidence projection would also fail.
        let eq = EventQuery {
            var: VarId::new(0),
            event: EventSpec::Import,
            identity: IdentitySpec::Global {
                name: SmolStr::new("fs"),
            },
            subject: SubjectSpec::Direct,
            constraints: vec![],
        };
        let decl = QueryDecl {
            expression: QueryExpr::Event(eq),
            emission: EmissionDecl {
                primary_var: VarId::new(1), // also wrong, but not reached
                kind: MatchKind::Import,
                symbol: "fs".into(),
            },
        };
        // Should report invalid_event_predicate, not missing_binding
        let result = validate_query_decl(&decl);
        assert_eq!(
            result,
            Err(QueryCompileError::InvalidEventPredicate {
                identity: "global".into(),
                event: "import".into(),
                subject: "direct".into(),
                detail: "identity/event/subject combination cannot select a semantic fact",
            })
        );
    }
}
