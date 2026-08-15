#[cfg(test)]
use super::PhysicalPlan;
use super::{PhysicalPlanValidationError, PhysicalRoot, RootBudget};
use crate::api::compiler::{
    normalized::{NormalizedEvent, NormalizedLifecycle, NormalizedQuery, NormalizedRoot},
    object_flow::CompiledObjectFlow,
    rule::{EvidenceDescriptor, IdentityConstraint},
};

/// Plan a normalized query into a [`PhysicalPlan`].
#[cfg(test)]
pub(crate) fn plan_normalized(
    nq: &NormalizedQuery,
) -> Result<PhysicalPlan, PhysicalPlanValidationError> {
    let mut budget = RootBudget::new();
    let mut roots = Vec::new();
    plan_normalized_roots_into(nq, &mut budget, &mut roots)?;
    PhysicalPlan::from_planned_roots(roots.into_boxed_slice())
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
    let relation =
        crate::api::compiler::validate::classify_subject_relation(event.event(), event.subject())
            .map_err(|_| PhysicalPlanValidationError::ImpossibleDimensions)?;

    match relation {
        crate::api::compiler::validate::SubjectRelation::Direct { identity } => {
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
        crate::api::compiler::validate::SubjectRelation::Returned {
            producer,
            object_slot,
            member,
            event,
        } => PhysicalRoot::returned_subject(
            IdentityConstraint::from(producer),
            object_slot,
            member.clone(),
            event.clone(),
            evidence.clone(),
        ),
        crate::api::compiler::validate::SubjectRelation::Instance {
            constructor,
            object_slot,
            member,
        } => PhysicalRoot::instance_subject(
            IdentityConstraint::from(constructor),
            object_slot,
            member.clone(),
            evidence.clone(),
        ),
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
