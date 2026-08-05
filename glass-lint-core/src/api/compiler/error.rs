use std::fmt;

use crate::api::rule::query::limits;

/// Validation failure for an executable physical plan.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum PhysicalPlanValidationError {
    ImpossibleDimensions,
    ConstraintsRequireCallEvent,
    NonCanonicalConstraints,
    UnavailablePrimaryEvidence,
    InvalidLifecycleRoot,
    ExcessiveLifecycleEvidence { requirements: usize, sinks: usize },
    RequirementsMismatch,
    ExcessiveArgumentGroups(usize),
    ExcessivePredicateCount(usize),
    ExcessiveAlternatives(usize),
}

impl fmt::Display for PhysicalPlanValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImpossibleDimensions => {
                f.write_str("identity/event/subject dimensions cannot select a semantic fact")
            }
            Self::ConstraintsRequireCallEvent => {
                f.write_str("argument constraints require a call-bearing event")
            }
            Self::NonCanonicalConstraints => {
                f.write_str("constraints are not in canonical grouped order")
            }
            Self::UnavailablePrimaryEvidence => f.write_str("primary evidence symbol is empty"),
            Self::InvalidLifecycleRoot => f.write_str("lifecycle root is malformed"),
            Self::ExcessiveLifecycleEvidence {
                requirements,
                sinks,
            } => write!(
                f,
                "lifecycle evidence has {requirements} requirements and {sinks} sinks, exceeding the indexed bound"
            ),
            Self::RequirementsMismatch => {
                f.write_str("physical roots and executable requirements disagree")
            }
            Self::ExcessiveArgumentGroups(count) => write!(
                f,
                "argument group count {count} exceeds limit {}",
                limits::MAX_ARGUMENT_GROUPS
            ),
            Self::ExcessivePredicateCount(count) => write!(
                f,
                "predicate count {count} exceeds limit {}",
                limits::MAX_PREDICATES_PER_ARGUMENT
            ),
            Self::ExcessiveAlternatives(count) => write!(
                f,
                "static alternative count {count} exceeds limit {}",
                limits::MAX_STATIC_ALTERNATIVES
            ),
        }
    }
}
