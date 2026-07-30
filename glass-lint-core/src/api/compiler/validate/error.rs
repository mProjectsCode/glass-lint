use crate::api::rule::query::{EventSpec, IdentitySpec, VarId};

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
    /// Contradictory predicates on the same variable or argument.
    ContradictoryPredicate {
        variable: VarId,
        detail: ContradictionKind,
    },
    /// `Any` branches produce incompatible types for the projected variable.
    IncompatibleBranchOutput {
        var: VarId,
        type_a: &'static str,
        type_b: &'static str,
    },
    /// Primary variable lacks an available source location.
    UnavailablePrimaryLocation { var: VarId },
    /// Lifecycle structure is invalid.
    InvalidLifecycle { detail: String },
    /// Query shape cannot be bounded at compile time.
    UnboundedQuery { detail: &'static str },
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
            Self::ContradictoryPredicate { .. } => "contradictory_predicate",
            Self::IncompatibleBranchOutput { .. } => "incompatible_branch_output",
            Self::UnavailablePrimaryLocation { .. } => "unavailable_primary_location",
            Self::InvalidLifecycle { .. } => "invalid_lifecycle",
            Self::UnboundedQuery { .. } => "unbounded_query",
            Self::InternalInvariant { .. } => "internal_invariant",
        }
    }
}

/// Classification of a contradiction between two predicates on the same
/// variable or argument.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ContradictionKind {
    EventKind,
    StrictIdentity,
    SubjectRelation,
    StaticExactValues,
    StaticExactAndPrefix,
    EvidenceProjection,
}

impl std::fmt::Display for ContradictionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EventKind => f.write_str("incompatible event kinds"),
            Self::StrictIdentity => f.write_str("incompatible strict identities"),
            Self::SubjectRelation => f.write_str("incompatible subject relationships"),
            Self::StaticExactValues => f.write_str("disjoint exact static-string values"),
            Self::StaticExactAndPrefix => {
                f.write_str("exact and prefix values that cannot both match")
            }
            Self::EvidenceProjection => f.write_str("incompatible evidence projections"),
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
            Self::ContradictoryPredicate { variable, detail } => {
                write!(
                    f,
                    "contradictory predicate on variable {variable}: {detail}"
                )
            }
            Self::IncompatibleBranchOutput {
                var,
                type_a,
                type_b,
            } => {
                write!(
                    f,
                    "variable {var} has incompatible types across Any branches: `{type_a}` vs `{type_b}`"
                )
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
            Self::InternalInvariant { detail } => {
                write!(f, "internal invariant: {detail}")
            }
        }
    }
}

// ── Dimension compatibility helpers ──────────────────────────────────────

/// Check whether the identity/event/subject combination is compatible for
/// a direct subject relationship (the most common case).
pub(crate) fn is_direct_dimension_valid(identity: &IdentitySpec, event: &EventSpec) -> bool {
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
/// For direct events, the event's identity member chain must match the
/// identity's path for heuristic and rooted members.
pub(crate) fn is_subject_identity_consistent(identity: &IdentitySpec, event: &EventSpec) -> bool {
    match (identity, event) {
        (
            IdentitySpec::Heuristic { name },
            EventSpec::MemberCall { member } | EventSpec::MemberRead { member },
        ) => member.eq_chain(name),
        (
            IdentitySpec::Rooted { path },
            EventSpec::MemberCall { member } | EventSpec::MemberRead { member },
        ) => *path == *member,
        _ => true,
    }
}

/// Top-level dimension check: is this (identity, event) valid?
pub(crate) fn is_valid_identity_event_pair(identity: &IdentitySpec, event: &EventSpec) -> bool {
    if !is_subject_identity_consistent(identity, event) {
        return false;
    }
    is_direct_dimension_valid(identity, event)
}

/// Check if an identity name or pattern is empty.
pub(crate) fn is_identity_empty(identity: &IdentitySpec) -> bool {
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
pub(crate) fn event_supports_constraints(event: &EventSpec) -> bool {
    matches!(event, EventSpec::Call | EventSpec::MemberCall { .. })
}
