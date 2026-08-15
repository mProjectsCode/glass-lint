use glass_lint_datastructures::SymbolPath;
use smol_str::SmolStr;

use crate::api::{
    compiler::normalized::{NormalizedSubject, ObjectSlot},
    rule::query::{EventSpec, IdentitySpec, VarId},
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
    /// Internal same-event merger state was incomplete at its sealing boundary.
    IncompleteSameEvent { missing: &'static str },
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
            Self::IncompleteSameEvent { .. } => "incomplete_same_event",
            Self::InternalInvariant { .. } => "internal_invariant",
        }
    }
}

/// Classification of a contradiction between two predicates on the same
/// variable or argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContradictionKind {
    EventKind,
    StrictIdentity,
    SubjectRelation,
    StaticExactValues,
    StaticExactAndPrefix,
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
            Self::IncompleteSameEvent { missing } => {
                write!(f, "incomplete same-event merge: missing {missing}")
            }
            Self::InternalInvariant { detail } => {
                write!(f, "internal invariant: {detail}")
            }
        }
    }
}

// ── Dimension compatibility helpers ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubjectRelationError {
    DirectIdentityEvent,
    ReturnedRequiresRootedMember,
    InstanceRequiresModuleCall,
    EmptySubjectIdentity,
    InvalidLifecycleSource,
}

impl SubjectRelationError {
    pub(crate) fn detail(self) -> &'static str {
        match self {
            Self::DirectIdentityEvent => "identity/event combination cannot select a semantic fact",
            Self::ReturnedRequiresRootedMember => {
                "returned subject requires a rooted member call or read"
            }
            Self::InstanceRequiresModuleCall => {
                "instance subject requires a module-export member call"
            }
            Self::EmptySubjectIdentity => "subject identity is empty",
            Self::InvalidLifecycleSource => {
                "lifecycle source must be a global call or rooted member call"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubjectRelation<'a> {
    Direct {
        identity: &'a IdentitySpec,
    },
    Returned {
        producer: &'a IdentitySpec,
        object_slot: ObjectSlot,
        member: &'a SymbolPath,
        event: &'a EventSpec,
    },
    Instance {
        constructor: &'a IdentitySpec,
        object_slot: ObjectSlot,
        member: &'a SymbolPath,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LifecycleSource<'a> {
    GlobalCall { name: &'a SmolStr },
    RootedMember { member: &'a SymbolPath },
}

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
            EventSpec::MemberCall { .. }
                | EventSpec::MemberRead { .. }
                | EventSpec::PropertyWrite { .. }
                | EventSpec::Construct
        ) | (
            IdentitySpec::ModuleExport { .. }
                | IdentitySpec::PackageModuleExport { .. }
                | IdentitySpec::Heuristic { .. },
            EventSpec::ClassReference
        ) | (
            IdentitySpec::LiteralString { .. } | IdentitySpec::PackageSpecifier { .. },
            EventSpec::Import | EventSpec::StringReference
        ) | (
            IdentitySpec::PrivateNetworkAddress,
            EventSpec::StringReference
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
            EventSpec::MemberCall { member }
            | EventSpec::MemberRead { member }
            | EventSpec::PropertyWrite { property: member },
        ) => member.eq_chain(name),
        (
            IdentitySpec::Rooted { path },
            EventSpec::MemberCall { member }
            | EventSpec::MemberRead { member }
            | EventSpec::PropertyWrite { property: member },
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

/// Validate the complete normalized subject relationship before lowering.
/// This is the single compatibility matrix for direct, returned, and
/// constructed-member event roots.
pub(crate) fn validate_subject_relation(
    event: &EventSpec,
    subject: &NormalizedSubject,
) -> Result<(), SubjectRelationError> {
    classify_subject_relation(event, subject).map(|_| ())
}

pub(crate) fn classify_subject_relation<'a>(
    event: &'a EventSpec,
    subject: &'a NormalizedSubject,
) -> Result<SubjectRelation<'a>, SubjectRelationError> {
    match subject {
        NormalizedSubject::Direct { identity } => {
            if is_valid_identity_event_pair(identity, event) {
                Ok(SubjectRelation::Direct { identity })
            } else {
                Err(SubjectRelationError::DirectIdentityEvent)
            }
        }
        NormalizedSubject::Returned {
            producer,
            object_slot,
        } => {
            if is_identity_empty(producer) {
                return Err(SubjectRelationError::EmptySubjectIdentity);
            }
            match (producer, event) {
                (
                    IdentitySpec::Rooted { .. },
                    EventSpec::MemberCall { member } | EventSpec::MemberRead { member },
                ) if !member.is_empty() => Ok(SubjectRelation::Returned {
                    producer,
                    object_slot: *object_slot,
                    member,
                    event,
                }),
                _ => Err(SubjectRelationError::ReturnedRequiresRootedMember),
            }
        }
        NormalizedSubject::Instance {
            constructor,
            object_slot,
        } => {
            if is_identity_empty(constructor) {
                return Err(SubjectRelationError::EmptySubjectIdentity);
            }
            match (constructor, event) {
                (
                    IdentitySpec::ModuleExport { .. } | IdentitySpec::PackageModuleExport { .. },
                    EventSpec::MemberCall { member },
                ) if !member.is_empty() => Ok(SubjectRelation::Instance {
                    constructor,
                    object_slot: *object_slot,
                    member,
                }),
                _ => Err(SubjectRelationError::InstanceRequiresModuleCall),
            }
        }
    }
}

pub(crate) fn classify_lifecycle_source<'a>(
    identity: &'a IdentitySpec,
    event: &'a EventSpec,
) -> Result<LifecycleSource<'a>, SubjectRelationError> {
    match (identity, event) {
        (IdentitySpec::Global { name }, EventSpec::Call) if !name.is_empty() => {
            Ok(LifecycleSource::GlobalCall { name })
        }
        (IdentitySpec::Rooted { path }, EventSpec::MemberCall { member })
            if !path.is_empty() && !member.is_empty() =>
        {
            Ok(LifecycleSource::RootedMember { member })
        }
        _ => Err(SubjectRelationError::InvalidLifecycleSource),
    }
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
        IdentitySpec::PrivateNetworkAddress => false,
    }
}

/// Check whether constraints are on a call-bearing event.
pub(crate) fn event_supports_constraints(event: &EventSpec) -> bool {
    event.supports_arguments()
}
