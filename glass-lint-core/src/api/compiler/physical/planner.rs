use super::{PhysicalPlanValidationError, PhysicalRoot, RootBudget};
#[cfg(test)]
use crate::api::compiler::CompiledMatcherPlan;
use crate::api::compiler::{
    normalized::{NormalizedEvent, NormalizedLifecycle, NormalizedQuery, NormalizedRoot},
    object_flow::CompiledObjectFlow,
    rule::{EvidenceDescriptor, IdentityConstraint},
};

/// Plan a normalized query into a [`CompiledMatcherPlan`].
#[cfg(test)]
pub(crate) fn plan_normalized(
    nq: &NormalizedQuery,
) -> Result<CompiledMatcherPlan, PhysicalPlanValidationError> {
    let mut budget = RootBudget::new();
    let mut roots = Vec::new();
    plan_normalized_roots_into(nq, &mut budget, &mut roots)?;
    CompiledMatcherPlan::from_planned_roots(roots.into_boxed_slice())
}

pub(crate) fn plan_normalized_roots_into(
    nq: &NormalizedQuery,
    budget: &mut RootBudget,
    roots: &mut Vec<PhysicalRoot>,
) -> Result<(), PhysicalPlanValidationError> {
    let evidence = EvidenceDescriptor::from(nq.emission());
    plan_root(nq.root(), &evidence, budget, roots)
}

fn plan_root(
    root: &NormalizedRoot,
    evidence: &EvidenceDescriptor,
    budget: &mut RootBudget,
    roots: &mut Vec<PhysicalRoot>,
) -> Result<(), PhysicalPlanValidationError> {
    match root {
        NormalizedRoot::Event(event) => {
            budget.reserve()?;
            roots.push(plan_event(event, evidence)?);
        }
        NormalizedRoot::Any(branches) => {
            for branch in branches {
                plan_root(branch, evidence, budget, roots)?;
            }
        }
        NormalizedRoot::Lifecycle(lifecycle) => {
            budget.reserve()?;
            roots.push(plan_lifecycle(lifecycle, &evidence.symbol)?);
        }
    }
    Ok(())
}

fn plan_event(
    event: &NormalizedEvent,
    evidence: &EvidenceDescriptor,
) -> Result<PhysicalRoot, PhysicalPlanValidationError> {
    match (event.subject(), event.event()) {
        (crate::api::compiler::normalized::NormalizedSubject::Direct { identity }, _) => {
            if event.arguments().is_empty() {
                Ok(PhysicalRoot::indexed_scan(
                    IdentityConstraint::from(identity),
                    event.event().clone(),
                    evidence.clone(),
                ))
            } else {
                Ok(PhysicalRoot::constrained_scan(
                    IdentityConstraint::from(identity),
                    event.event().clone(),
                    event.arguments().clone(),
                    evidence.clone(),
                ))
            }
        }
        (
            crate::api::compiler::normalized::NormalizedSubject::Returned {
                producer,
                object_slot,
            },
            crate::api::rule::query::EventSpec::MemberCall { member }
            | crate::api::rule::query::EventSpec::MemberRead { member },
        ) => PhysicalRoot::returned_subject(
            IdentityConstraint::from(producer),
            *object_slot,
            member.clone(),
            event.event().clone(),
            evidence.clone(),
        ),
        (
            crate::api::compiler::normalized::NormalizedSubject::Instance {
                constructor,
                object_slot,
            },
            crate::api::rule::query::EventSpec::MemberCall { member },
        ) => PhysicalRoot::instance_subject(
            IdentityConstraint::from(constructor),
            *object_slot,
            member.clone(),
            evidence.clone(),
        ),
        _ => Err(PhysicalPlanValidationError::ImpossibleDimensions),
    }
}

fn plan_lifecycle(
    lifecycle: &NormalizedLifecycle,
    symbol: &str,
) -> Result<PhysicalRoot, PhysicalPlanValidationError> {
    CompiledObjectFlow::from_normalized_lifecycle(lifecycle, symbol)
        .map(|flow| PhysicalRoot::Lifecycle { flow })
        .map_err(
            |error| PhysicalPlanValidationError::InvalidLifecycleSource {
                detail: error.detail(),
            },
        )
}
