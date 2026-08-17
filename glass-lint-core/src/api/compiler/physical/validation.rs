use super::{PhysicalPlanValidationError, PhysicalRoot};
#[cfg(test)]
use crate::api::compiler::CompiledMatcherPlan;
use crate::api::{
    compiler::{normalized::CanonicalArgumentConstraints, requirements::PlanRequirements},
    rule::{
        ArgumentIndex, ArgumentMatcher, ArgumentMatcherKind, StaticStringPredicateKind,
        ValueMatcherKind, query::limits,
    },
};

#[cfg(test)]
pub(crate) fn validate_physical_plan(
    plan: &CompiledMatcherPlan,
) -> Result<(), PhysicalPlanValidationError> {
    for root in plan.roots() {
        root.validate()?;
    }
    if requirements_for_roots(plan.roots()) != *plan.requirements() {
        return Err(PhysicalPlanValidationError::RequirementsMismatch);
    }
    Ok(())
}

pub(super) fn requirements_for_roots(roots: &[PhysicalRoot]) -> PlanRequirements {
    let mut requirements = PlanRequirements::default();
    for root in roots {
        root.merge_requirements_into(&mut requirements);
    }
    requirements
}

/// Validate that compiled constraints are well-formed.
pub(super) fn validate_canonical_constraints(
    constraints: &CanonicalArgumentConstraints,
) -> Result<(), PhysicalPlanValidationError> {
    let groups = constraints.groups();
    if groups.is_empty() {
        return Err(PhysicalPlanValidationError::NonCanonicalConstraints);
    }
    if groups.len() > limits::MAX_ARGUMENT_GROUPS {
        return Err(PhysicalPlanValidationError::ExcessiveArgumentGroups(
            groups.len(),
        ));
    }

    let mut previous_index: Option<ArgumentIndex> = None;
    for group in groups {
        if group.predicates().is_empty() {
            return Err(PhysicalPlanValidationError::NonCanonicalConstraints);
        }
        if group.predicates().len() > limits::MAX_PREDICATES_PER_ARGUMENT {
            return Err(PhysicalPlanValidationError::ExcessivePredicateCount(
                group.predicates().len(),
            ));
        }
        if let Some(previous) = previous_index
            && previous >= group.index()
        {
            return Err(PhysicalPlanValidationError::NonCanonicalConstraints);
        }
        previous_index = Some(group.index());
        for matcher in group.predicates() {
            if let Some(count) = count_matcher_alternatives(matcher)
                && count > limits::MAX_STATIC_ALTERNATIVES
            {
                return Err(PhysicalPlanValidationError::ExcessiveAlternatives(count));
            }
        }
    }
    Ok(())
}

fn count_matcher_alternatives(matcher: &ArgumentMatcher) -> Option<usize> {
    match matcher.kind() {
        ArgumentMatcherKind::Value(value) => match value.kind() {
            ValueMatcherKind::StaticString(predicate) => match predicate.kind() {
                StaticStringPredicateKind::Exact(values)
                | StaticStringPredicateKind::Prefix(values)
                | StaticStringPredicateKind::ContainsAny(values)
                | StaticStringPredicateKind::ContainsAll(values) => Some(values.len()),
                StaticStringPredicateKind::Any => None,
            },
            ValueMatcherKind::Any => None,
        },
        _ => None,
    }
}
