//! Project-level flow projection over qualified function effects.
//!
//! This pass is deliberately small and bounded. Local object flow remains the
//! source of truth for one module; this overlay carries a proven object state
//! through parameter-to-call relations and qualified call edges.

mod evidence;
mod graph;
mod propagation;
mod sources;
mod state;
mod worklist;

use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::Budget;
use hashbrown::HashMap;

use crate::{
    analysis::{
        ProjectSemanticModel,
        flow::{
            FlowCompletion, FlowCompletionReason,
            cross::{
                evidence::ModuleEvidence, graph::QualifiedCallGraph, sources::FlowSources,
                state::CallContext, worklist::ContextWorklist,
            },
            planning::{BoundFlowPlan, BoundLifecycleRoot},
        },
        model::flow::FlowId,
        project::state::LinkingSession,
        trace::TraceArena,
    },
    api::{classification::RuleEvidenceCapacity, compiler::CompiledObjectFlow},
    project::ModuleId,
};

const MAX_CONTEXTS: usize = 65_536;
const MAX_PENDING: usize = 65_536;

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::analysis) struct CrossProjectionOutcome {
    pub(in crate::analysis) completion: FlowCompletion,
    pub(in crate::analysis) projections: usize,
    pub(in crate::analysis) operations: usize,
    pub(in crate::analysis) trace_heads: usize,
}

/// Shared state for one bounded cross-file projection session.
struct CrossProjectionSession<'a> {
    project: &'a ProjectSemanticModel,
    evidence: &'a mut HashMap<ModuleId, ModuleEvidence>,
    call_graph: &'a QualifiedCallGraph,
    worklist: &'a mut ContextWorklist,
    names: &'a glass_lint_datastructures::NameTable,
    arena: &'a mut TraceArena,
}

struct CrossWorklist<'a, 'arena> {
    project: &'a ProjectSemanticModel,
    roots: &'a [BoundLifecycleRoot<'a>],
    flows: HashMap<FlowId, &'a CompiledObjectFlow>,
    evidence: HashMap<ModuleId, ModuleEvidence>,
    call_graph: QualifiedCallGraph,
    worklist: ContextWorklist,
    flow_plans: HashMap<ModuleId, BoundFlowPlan<'a>>,
    step_budget: Budget,
    arena: &'arena mut TraceArena,
    projections: usize,
}

impl CrossWorklist<'_, '_> {
    fn run(&mut self) -> FlowCompletion {
        if self.worklist.is_exhausted() {
            return FlowCompletion::incomplete(FlowCompletionReason::CrossContextLimit);
        }
        while let Some(context) = self.worklist.pop_front() {
            self.projections = self.projections.saturating_add(1);
            if !self.step_budget.try_push() {
                return FlowCompletion::incomplete(FlowCompletionReason::CrossStepBudget);
            }
            self.project_context(&context);
            if self.worklist.is_exhausted() {
                return FlowCompletion::incomplete(FlowCompletionReason::CrossContextLimit);
            }
        }
        FlowCompletion::default()
    }

    fn project_context(&mut self, context: &CallContext) {
        let Some(effect) = self
            .project
            .effect(crate::analysis::QualifiedFunctionId::new(
                context.module(),
                context.function(),
            ))
        else {
            return;
        };
        if effect.is_invalid() {
            return;
        }
        let Some(flow) = self.flows.get(&context.state().flow_id()).copied() else {
            return;
        };
        let Some(names) = self.project.module_names(context.module()) else {
            return;
        };
        let flow_plan = self
            .flow_plans
            .entry(context.module())
            .or_insert_with(|| BoundFlowPlan::new(self.roots, names));
        let mut session = CrossProjectionSession {
            project: self.project,
            evidence: &mut self.evidence,
            call_graph: &self.call_graph,
            worklist: &mut self.worklist,
            names,
            arena: self.arena,
        };
        let mut state = context.state().clone();
        let mut propagated = BTreeSet::new();
        propagation::UsageProjector::new(
            &mut session,
            context,
            effect,
            flow,
            flow_plan,
            &mut state,
            &mut propagated,
        )
        .project();
        propagation::CallPropagation::new(
            &mut session,
            effect,
            context,
            &mut propagated,
            None,
            &state,
        )
        .propagate();
    }

    fn finish(
        mut self,
        mut completion: FlowCompletion,
        source_completion: FlowCompletion,
    ) -> (
        BTreeMap<ModuleId, crate::api::classification::RuleEvidenceTable>,
        CrossProjectionOutcome,
    ) {
        completion.merge(source_completion);
        if completion.is_incomplete() {
            for module_evidence in self.evidence.values_mut() {
                module_evidence.mark_all_possible();
            }
        }
        let trace_heads = self
            .evidence
            .values()
            .map(evidence::ModuleEvidence::trace_heads)
            .sum();
        let output = self
            .evidence
            .into_iter()
            .map(|(id, module)| (id, module.into_evidence()))
            .collect();
        (
            output,
            CrossProjectionOutcome {
                completion,
                projections: self.projections,
                operations: self.step_budget.used(),
                trace_heads,
            },
        )
    }
}

pub(in crate::analysis) fn collect(
    project: &ProjectSemanticModel,
    roots: &[BoundLifecycleRoot<'_>],
    capacity: RuleEvidenceCapacity,
    session: &mut LinkingSession,
    arena: &mut TraceArena,
) -> (
    BTreeMap<ModuleId, crate::api::classification::RuleEvidenceTable>,
    CrossProjectionOutcome,
) {
    let flows = roots
        .iter()
        .map(|root| (root.flow_id(), root.flow()))
        .collect::<HashMap<_, _>>();
    let evidence = project
        .modules()
        .map(|module| (module.id(), ModuleEvidence::new(capacity)))
        .collect::<HashMap<_, _>>();
    if flows.is_empty() {
        let empty = evidence
            .into_iter()
            .map(|(id, m)| (id, m.into_evidence()))
            .collect();
        return (empty, CrossProjectionOutcome::default());
    }

    let call_graph = QualifiedCallGraph::build(project, session);
    // Cross flow is bounded by the same operations budget as local flow.
    let operation_limit = project.flow_limit();
    let mut source_budget = Budget::new(operation_limit);
    let (sources, source_completion) =
        FlowSources::collect(project, &flows, &call_graph, &mut source_budget);
    let worklist = ContextWorklist::seed(project, &sources, &call_graph);
    let step_budget = Budget::new(operation_limit);
    let mut collector = CrossWorklist {
        project,
        roots,
        flows,
        evidence,
        call_graph,
        worklist,
        flow_plans: HashMap::new(),
        step_budget,
        arena,
        projections: 0,
    };
    let completion = collector.run();
    collector.finish(completion, source_completion)
}

#[cfg(test)]
mod tests;
