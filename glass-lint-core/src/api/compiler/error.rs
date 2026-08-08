use crate::api::rule::PhysicalPlanDiagnostic;

/// Validation failure for an executable physical plan.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum PhysicalPlanValidationError {
    ImpossibleDimensions,
    ConstraintsRequireCallEvent,
    NonCanonicalConstraints,
    UnavailablePrimaryEvidence,
    InvalidLifecycleRoot,
    InvalidLifecycleSource {
        detail: &'static str,
    },
    ExcessiveLifecycleEvidence {
        requirements: usize,
        sinks: usize,
    },
    #[cfg(test)]
    RequirementsMismatch,
    ExcessiveArgumentGroups(usize),
    ExcessivePredicateCount(usize),
    ExcessiveAlternatives(usize),
}

impl From<PhysicalPlanValidationError> for PhysicalPlanDiagnostic {
    fn from(error: PhysicalPlanValidationError) -> Self {
        match error {
            PhysicalPlanValidationError::ImpossibleDimensions => Self::ImpossibleDimensions,
            PhysicalPlanValidationError::ConstraintsRequireCallEvent => {
                Self::ConstraintsRequireCallEvent
            }
            PhysicalPlanValidationError::NonCanonicalConstraints => Self::NonCanonicalConstraints,
            PhysicalPlanValidationError::UnavailablePrimaryEvidence => {
                Self::UnavailablePrimaryEvidence
            }
            PhysicalPlanValidationError::InvalidLifecycleRoot => Self::InvalidLifecycleRoot,
            PhysicalPlanValidationError::InvalidLifecycleSource { detail } => {
                Self::InvalidLifecycleSource {
                    detail: detail.to_owned(),
                }
            }
            PhysicalPlanValidationError::ExcessiveLifecycleEvidence {
                requirements,
                sinks,
            } => Self::ExcessiveLifecycleEvidence {
                requirements,
                sinks,
            },
            #[cfg(test)]
            PhysicalPlanValidationError::RequirementsMismatch => {
                unreachable!("test-only malformed plan error has no public diagnostic")
            }
            PhysicalPlanValidationError::ExcessiveArgumentGroups(count) => {
                Self::ExcessiveArgumentGroups(count)
            }
            PhysicalPlanValidationError::ExcessivePredicateCount(count) => {
                Self::ExcessivePredicateCount(count)
            }
            PhysicalPlanValidationError::ExcessiveAlternatives(count) => {
                Self::ExcessiveAlternatives(count)
            }
        }
    }
}
