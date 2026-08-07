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
        facts::FactId,
        flow::{
            cross::{
                evidence::ModuleEvidence,
                graph::QualifiedCallGraph,
                sources::FlowSources,
                state::{CallContext, CrossFlowState},
                worklist::ContextWorklist,
            },
            effect::FunctionEffect,
            planning::BoundFlowPlan,
        },
        model::flow::{FlowId, FlowLimits},
        project::state::LinkingSession,
        trace::TraceArena,
    },
    api::compiler::{CompiledObjectFlow, CompiledRuleSelection},
    project::ModuleId,
};

const MAX_CONTEXTS: usize = 65_536;
const MAX_PENDING: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FlowPlanKey {
    flow: FlowId,
    module: ModuleId,
}

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::analysis) struct CrossProjectionOutcome {
    pub(in crate::analysis) exhausted: bool,
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

/// Inputs for one bounded worklist context projection.
struct ContextProjection<'a, 'session> {
    session: &'a mut CrossProjectionSession<'session>,
    context: &'a CallContext,
    effect: &'a FunctionEffect,
    flow: &'a CompiledObjectFlow,
    flow_plan: &'a BoundFlowPlan<'session>,
    state: &'a CrossFlowState,
}

impl ContextProjection<'_, '_> {
    fn project(&mut self) {
        let mut current_state = self.state.clone();
        let mut propagated_calls = BTreeSet::<FactId>::new();
        propagation::UsageProjector {
            session: self.session,
            context: self.context,
            effect: self.effect,
            flow: self.flow,
            flow_plan: self.flow_plan,
            state: &mut current_state,
            propagated: &mut propagated_calls,
        }
        .project();
        propagation::CallPropagation {
            session: self.session,
            effect: self.effect,
            module: self.context.module(),
            context: self.context,
            propagated: &mut propagated_calls,
            through: None,
            state: &current_state,
        }
        .propagate();
    }
}

enum WorklistStop {
    Drained,
    StepBudgetExhausted,
    ContextLimit,
}

struct CrossWorklist<'a, 'arena> {
    project: &'a ProjectSemanticModel,
    flows: HashMap<FlowId, &'a CompiledObjectFlow>,
    evidence: HashMap<ModuleId, ModuleEvidence>,
    call_graph: QualifiedCallGraph,
    worklist: ContextWorklist,
    flow_plan_cache: HashMap<FlowPlanKey, BoundFlowPlan<'a>>,
    step_budget: Budget,
    arena: &'arena mut TraceArena,
    projections: usize,
}

impl CrossWorklist<'_, '_> {
    fn run(&mut self) -> WorklistStop {
        if self.worklist.is_exhausted() {
            return WorklistStop::ContextLimit;
        }
        while let Some(context) = self.worklist.pop_front() {
            self.projections = self.projections.saturating_add(1);
            if !self.step_budget.try_push() {
                return WorklistStop::StepBudgetExhausted;
            }
            self.project_context(&context);
            if self.worklist.is_exhausted() {
                return WorklistStop::ContextLimit;
            }
        }
        WorklistStop::Drained
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
            .flow_plan_cache
            .entry(FlowPlanKey {
                flow: context.state().flow_id(),
                module: context.module(),
            })
            .or_insert_with(|| BoundFlowPlan::single(context.state().flow_id(), flow, names));
        let mut session = CrossProjectionSession {
            project: self.project,
            evidence: &mut self.evidence,
            call_graph: &self.call_graph,
            worklist: &mut self.worklist,
            names,
            arena: self.arena,
        };
        ContextProjection {
            session: &mut session,
            context,
            effect,
            flow,
            flow_plan,
            state: context.state(),
        }
        .project();
    }

    fn finish(
        mut self,
        stop: &WorklistStop,
        return_budget_exhausted: bool,
    ) -> (
        BTreeMap<ModuleId, crate::api::classification::RuleEvidenceTable>,
        CrossProjectionOutcome,
    ) {
        let exhausted = return_budget_exhausted || !matches!(stop, WorklistStop::Drained);
        if exhausted {
            for module_evidence in self.evidence.values_mut() {
                module_evidence.clear();
            }
        }
        let trace_heads = self
            .evidence
            .values()
            .map(|module| module.trace_heads)
            .sum();
        let output = self
            .evidence
            .into_iter()
            .map(|(id, module)| (id, module.into_evidence()))
            .collect();
        (
            output,
            CrossProjectionOutcome {
                exhausted,
                projections: self.projections,
                operations: self.step_budget.used(),
                trace_heads,
            },
        )
    }
}

pub(in crate::analysis) fn collect(
    project: &ProjectSemanticModel,
    matchers: &CompiledRuleSelection<'_>,
    session: &mut LinkingSession,
    arena: &mut TraceArena,
) -> (
    BTreeMap<ModuleId, crate::api::classification::RuleEvidenceTable>,
    CrossProjectionOutcome,
) {
    let flows = collect_flows(matchers);
    let capacity = matchers.evidence_capacity();
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
    let operation_limit = FlowLimits::from_flow_operations(project.flow_limit()).operation_limit();
    let mut source_budget = Budget::new(operation_limit);
    let (sources, return_budget_exhausted) =
        FlowSources::collect(project, &flows, &call_graph, &mut source_budget);
    let worklist = ContextWorklist::seed(project, &sources, &call_graph);
    let step_budget = Budget::new(operation_limit);
    let mut collector = CrossWorklist {
        project,
        flows,
        evidence,
        call_graph,
        worklist,
        flow_plan_cache: HashMap::new(),
        step_budget,
        arena,
        projections: 0,
    };
    let stop = collector.run();
    collector.finish(&stop, return_budget_exhausted)
}

fn collect_flows<'a>(
    matchers: &'a CompiledRuleSelection<'a>,
) -> HashMap<FlowId, &'a CompiledObjectFlow> {
    let mut flows = HashMap::new();
    for (rule_index, matcher) in matchers.selected_matchers() {
        let mut flow_index = 0usize;
        for root in matcher.physical_roots() {
            if let crate::api::compiler::physical::PhysicalRoot::Lifecycle { flow } = root {
                flows.insert(FlowId::new(rule_index, flow_index), flow);
                flow_index += 1;
            }
        }
    }
    flows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        analysis::{
            facts::FactId,
            flow::cross::{
                sources::{SourceCandidate, SourceKey},
                state::{CallContext, CrossFlowState, QualifiedEvent, SourceBudget},
            },
            model::flow::FlowId,
            value::{FunctionId, ValueId},
        },
        api::classification::RuleIndex,
        project::ModuleId,
    };

    fn key(module: u32, function: u32, value: u32) -> SourceKey {
        SourceKey::new(
            ModuleId::new(module),
            FunctionId::from_test(function),
            ValueId::from_test(value),
        )
    }

    fn candidate(rule: usize, flow: usize, fact: u32) -> SourceCandidate {
        SourceCandidate::new(
            FlowId::new(RuleIndex::new(rule), flow),
            FactId::from_test(fact),
        )
    }

    #[test]
    fn propagate_transfers_along_adjacency_edge() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add_candidate(from, candidate(0, 0, 10));
        sources.add_candidate(from, candidate(0, 0, 20));
        sources.add_edge(from, to);

        assert!(!sources.propagate(&mut budget));

        assert_eq!(sources.candidate_count(&to), 2);
        assert!(sources.contains_candidate(&to, candidate(0, 0, 10)));
        assert!(sources.contains_candidate(&to, candidate(0, 0, 20)));
    }

    #[test]
    fn propagate_deduplicates_by_construction() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add_candidate(from, candidate(0, 0, 10));
        sources.add_edge(from, to);

        assert!(!sources.propagate(&mut budget));
        assert_eq!(sources.candidate_count(&to), 1);

        // Second propagation is a no-op because candidates are already at the
        // destination.
        assert!(!sources.propagate(&mut budget));
        assert_eq!(sources.candidate_count(&to), 1);
    }

    #[test]
    fn propagate_partial_novelty() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add_candidate(from, candidate(0, 0, 10));
        sources.add_candidate(from, candidate(0, 0, 20));
        sources.add_candidate(to, candidate(0, 0, 10));
        sources.add_edge(from, to);

        assert!(!sources.propagate(&mut budget));
        assert_eq!(sources.candidate_count(&to), 2);

        assert!(!sources.propagate(&mut budget));
    }

    #[test]
    fn propagate_missing_source_is_no_op() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add_edge(from, to);

        assert!(!sources.propagate(&mut budget));
        assert!(!sources.has_candidates(&to));
        assert!(!sources.has_candidates(&from));
    }

    #[test]
    fn propagate_self_edge_is_skipped() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let k = key(1, 1, 1);
        sources.add_candidate(k, candidate(0, 0, 10));
        sources.add_edge(k, k);

        assert!(!sources.propagate(&mut budget));
        assert_eq!(sources.candidate_count(&k), 1);
    }

    #[test]
    fn propagate_multi_hop() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let a = key(1, 1, 1);
        let b = key(1, 1, 2);
        let c = key(1, 1, 3);

        sources.add_candidate(a, candidate(0, 0, 10));
        sources.add_edge(a, b);
        sources.add_edge(b, c);

        assert!(!sources.propagate(&mut budget));

        assert_eq!(sources.candidate_count(&b), 1);
        assert!(sources.contains_candidate(&b, candidate(0, 0, 10)));
        assert_eq!(sources.candidate_count(&c), 1);
        assert!(sources.contains_candidate(&c, candidate(0, 0, 10)));
    }

    #[test]
    fn propagate_multi_hop_converges() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let a = key(1, 1, 1);
        let b = key(1, 1, 2);

        sources.add_candidate(a, candidate(0, 0, 10));
        sources.add_edge(a, b);
        sources.add_edge(b, a);

        let exhausted = sources.propagate(&mut budget);
        assert!(!exhausted);
        assert!(sources.contains_candidate(&b, candidate(0, 0, 10)));
    }

    #[test]
    fn propagate_preserves_ordering_at_destination() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add_candidate(to, candidate(0, 0, 5));
        sources.add_candidate(from, candidate(0, 1, 20));
        sources.add_candidate(from, candidate(0, 0, 10));
        sources.add_edge(from, to);

        sources.propagate(&mut budget);

        let ordered: Vec<_> = sources.candidates(&to).copied().collect();
        assert_eq!(ordered[0], candidate(0, 0, 5));
        assert_eq!(ordered[1], candidate(0, 0, 10));
        assert_eq!(ordered[2], candidate(0, 1, 20));
    }

    #[test]
    fn propagate_pending_limit_exhausted() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let a = key(1, 1, 1);
        let b = key(1, 1, 2);
        for i in 0..(u32::try_from(MAX_PENDING).unwrap_or(u32::MAX) + 10) {
            sources.add_candidate(a, candidate(0, 0, i));
        }
        // a → b edges cause all candidates to flow into b in one round,
        // filling the pending queue past the safety limit.
        sources.add_edge(a, b);

        assert!(sources.propagate(&mut budget));
    }

    #[test]
    fn source_budget_transfer_limit_is_detected() {
        let mut budget = SourceBudget::new(10);
        for _ in 0..10 {
            assert!(budget.try_charge());
            assert!(!budget.exhausted());
        }
        assert!(!budget.try_charge());
        assert!(budget.exhausted());
    }

    #[test]
    fn source_budget_not_exhausted_after_stabilization() {
        let mut budget = SourceBudget::new(100);
        assert!(budget.try_charge());
        assert!(!budget.exhausted());
    }

    #[test]
    fn source_candidate_ordering_is_deterministic() {
        let mut sources = FlowSources::default();
        let k = key(1, 1, 1);

        sources.add_candidate(k, candidate(0, 2, 30));
        sources.add_candidate(k, candidate(0, 0, 10));
        sources.add_candidate(k, candidate(0, 1, 20));

        let ordered: Vec<_> = sources.candidates(&k).copied().collect();
        assert_eq!(ordered[0], candidate(0, 0, 10));
        assert_eq!(ordered[1], candidate(0, 1, 20));
        assert_eq!(ordered[2], candidate(0, 2, 30));
    }

    fn context(module: u32, function: u32) -> CallContext {
        CallContext::for_test(
            ModuleId::new(module),
            FunctionId::from_test(function),
            CrossFlowState::known(
                FlowId::new(RuleIndex::new(0), 0),
                QualifiedEvent::new(ModuleId::new(1), FactId::from_test(1)),
            ),
        )
    }

    #[test]
    fn worklist_len_counts_total_retained_not_pending() {
        let mut wl = ContextWorklist::new(10);
        assert_eq!(wl.len(), 0);

        // Push two contexts
        assert_eq!(wl.push(context(1, 1)), worklist::ContextAdmission::Inserted);
        assert_eq!(wl.len(), 1);
        assert_eq!(wl.push(context(1, 2)), worklist::ContextAdmission::Inserted);
        assert_eq!(wl.len(), 2);

        // Pop one — seen still retains both, so len is still 2
        let _popped = wl.pop_front();
        assert_eq!(wl.len(), 2);

        // Duplicate push does not increase retained count
        assert_eq!(
            wl.push(context(1, 1)),
            worklist::ContextAdmission::Duplicate
        );
        assert_eq!(wl.len(), 2);
    }

    #[test]
    fn worklist_respects_max_retained_limit() {
        let mut wl = ContextWorklist::new(3);
        assert_eq!(wl.push(context(1, 1)), worklist::ContextAdmission::Inserted);
        assert_eq!(wl.push(context(1, 2)), worklist::ContextAdmission::Inserted);
        assert_eq!(wl.push(context(1, 3)), worklist::ContextAdmission::Inserted);
        assert!(!wl.is_exhausted());
        // Fourth unique context hits the limit
        assert_eq!(wl.push(context(1, 4)), worklist::ContextAdmission::Full);
        assert_eq!(wl.len(), 3);
        assert!(wl.is_exhausted());
    }

    #[test]
    fn worklist_is_exhausted_false_below_limit() {
        let mut wl = ContextWorklist::new(5);
        assert!(!wl.is_exhausted());
        assert_eq!(wl.push(context(1, 1)), worklist::ContextAdmission::Inserted);
        assert!(!wl.is_exhausted());
    }
}
