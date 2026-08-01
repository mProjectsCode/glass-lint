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
                graph::{FlowPathPlan, QualifiedCallGraph},
                sources::FlowSources,
                state::{CallContext, CrossFlowState},
                worklist::ContextWorklist,
            },
            effect::FunctionEffect,
        },
        model::flow::{FlowId, FlowLimits},
        project::state::LinkingSession,
        trace::TraceArena,
    },
    api::{
        classification::ClassificationEvidence,
        compiler::{CompiledObjectFlow, CompiledRuleSelection},
    },
    project::ModuleId,
};

const MAX_CONTEXTS: usize = 65_536;
const MAX_PENDING: usize = 65_536;

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::analysis) struct CrossProjectionOutcome {
    pub(in crate::analysis) exhausted: bool,
    pub(in crate::analysis) projections: usize,
    pub(in crate::analysis) operations: usize,
    pub(in crate::analysis) trace_heads: usize,
}

/// Inputs for one bounded worklist context projection.
struct ContextProjection<'a> {
    project: &'a ProjectSemanticModel,
    evidence: &'a mut HashMap<ModuleId, ModuleEvidence>,
    context: &'a CallContext,
    effect: &'a FunctionEffect,
    flow: &'a CompiledObjectFlow,
    flow_plan: &'a FlowPathPlan,
    call_graph: &'a QualifiedCallGraph,
    state: &'a CrossFlowState,
    worklist: &'a mut ContextWorklist,
    names: &'a glass_lint_datastructures::NameTable,
    arena: &'a mut TraceArena,
}

impl ContextProjection<'_> {
    fn project(&mut self) {
        let mut current_state = self.state.clone();
        let mut propagated_calls = BTreeSet::<FactId>::new();
        propagation::UsageProjector {
            project: self.project,
            evidence: self.evidence,
            context: self.context,
            effect: self.effect,
            flow: self.flow,
            flow_plan: self.flow_plan,
            call_graph: self.call_graph,
            state: &mut current_state,
            propagated: &mut propagated_calls,
            worklist: self.worklist,
            names: self.names,
            arena: self.arena,
        }
        .project();
        propagation::CallPropagation {
            project: self.project,
            effect: self.effect,
            module: self.context.module,
            context: self.context,
            propagated: &mut propagated_calls,
            through: None,
            state: &current_state,
            worklist: self.worklist,
            call_graph: self.call_graph,
        }
        .propagate();
    }
}

#[allow(clippy::too_many_lines)]
pub(in crate::analysis) fn collect(
    project: &ProjectSemanticModel,
    matchers: &CompiledRuleSelection<'_>,
    session: &mut LinkingSession,
    arena: &mut TraceArena,
) -> (
    BTreeMap<ModuleId, Vec<Vec<ClassificationEvidence>>>,
    CrossProjectionOutcome,
) {
    // Single worklist loop: setup, iteration with UsageProjector and
    // CallPropagation per context, then final exhaustion handling.
    // Extracting the loop body would require passing 12+ context fields
    // through every call site.
    // Collect flows from lifecycle roots explicitly, keeping the
    // compiled plan root as the source of truth.  Each lifecycle root
    // embeds its CompiledObjectFlow directly.
    let mut flows = HashMap::<FlowId, &CompiledObjectFlow>::new();
    for (rule_index, matcher) in matchers.selected_matchers() {
        let mut flow_index = 0usize;
        for root in matcher.physical_roots() {
            if let crate::api::compiler::physical::PhysicalRoot::Lifecycle { flow } = root {
                flows.insert(FlowId::new(rule_index, flow_index), flow);
                flow_index += 1;
            }
        }
    }
    let rule_count = matchers.rule_capacity();
    let mut evidence = project
        .modules()
        .map(|module| (module.id(), ModuleEvidence::new(rule_count)))
        .collect::<HashMap<_, _>>();
    if flows.is_empty() {
        let empty = evidence
            .into_iter()
            .map(|(id, m)| (id, m.into_evidence()))
            .collect();
        return (empty, CrossProjectionOutcome::default());
    }

    let call_graph = QualifiedCallGraph::build(project, session);
    let mut source_budget =
        Budget::new(FlowLimits::from_flow_operations(project.flow_limit()).operation_limit());
    let (sources, return_budget_exhausted) =
        FlowSources::collect(project, &flows, &call_graph, &mut source_budget);
    let mut worklist = ContextWorklist::seed(project, &sources, &call_graph);

    let mut flow_plan_cache: HashMap<(FlowId, ModuleId), FlowPathPlan> = HashMap::new();

    let mut step_budget =
        Budget::new(FlowLimits::from_flow_operations(project.flow_limit()).operation_limit());
    let mut projections = 0usize;
    while let Some(context) = worklist.pop_front() {
        projections = projections.saturating_add(1);
        if !step_budget.try_push() {
            break;
        }
        let Some(effect) = project.effect(context.module, context.function) else {
            continue;
        };
        if effect.is_invalid() {
            continue;
        }
        let Some(flow) = flows.get(&context.state.flow).copied() else {
            continue;
        };
        let names = project
            .module_names(context.module)
            .expect("module has names");
        let flow_plan = flow_plan_cache
            .entry((context.state.flow, context.module))
            .or_insert_with(|| FlowPathPlan::build(flow, names));
        ContextProjection {
            project,
            evidence: &mut evidence,
            context: &context,
            effect,
            flow,
            flow_plan,
            call_graph: &call_graph,
            state: &context.state,
            worklist: &mut worklist,
            names,
            arena,
        }
        .project();
        if worklist.len() >= MAX_CONTEXTS {
            break;
        }
    }
    let exhausted =
        return_budget_exhausted || step_budget.exhausted() || worklist.len() >= MAX_CONTEXTS;
    if exhausted {
        for module_evidence in evidence.values_mut() {
            module_evidence.clear();
        }
    }
    let trace_heads = evidence.values().map(|module| module.trace_heads).sum();
    let output = evidence
        .into_iter()
        .map(|(id, m)| (id, m.into_evidence()))
        .collect();
    (
        output,
        CrossProjectionOutcome {
            exhausted,
            projections,
            operations: step_budget.used(),
            trace_heads,
        },
    )
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
            model::flow::{FlowId, RequirementSet},
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
        SourceCandidate {
            flow: FlowId::new(RuleIndex::new(rule), flow),
            fact: FactId::from_test(fact),
        }
    }

    #[test]
    fn propagate_transfers_along_adjacency_edge() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(from, candidate(0, 0, 10));
        sources.add(from, candidate(0, 0, 20));
        sources.adjacency.insert(from, vec![to]);

        assert!(!sources.propagate(&mut budget));

        let dest = sources.get(&to).unwrap();
        assert_eq!(dest.len(), 2);
        assert!(dest.contains(&candidate(0, 0, 10)));
        assert!(dest.contains(&candidate(0, 0, 20)));
    }

    #[test]
    fn propagate_deduplicates_by_construction() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(from, candidate(0, 0, 10));
        sources.adjacency.insert(from, vec![to]);

        assert!(!sources.propagate(&mut budget));
        assert_eq!(sources.get(&to).unwrap().len(), 1);

        // Second propagation is a no-op because candidates are already at the
        // destination.
        assert!(!sources.propagate(&mut budget));
        assert_eq!(sources.get(&to).unwrap().len(), 1);
    }

    #[test]
    fn propagate_partial_novelty() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(from, candidate(0, 0, 10));
        sources.add(from, candidate(0, 0, 20));
        sources.add(to, candidate(0, 0, 10));
        sources.adjacency.insert(from, vec![to]);

        assert!(!sources.propagate(&mut budget));
        assert_eq!(sources.get(&to).unwrap().len(), 2);

        assert!(!sources.propagate(&mut budget));
    }

    #[test]
    fn propagate_missing_source_is_no_op() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.adjacency.insert(from, vec![to]);

        assert!(!sources.propagate(&mut budget));
        assert!(sources.get(&to).is_none());
        assert!(sources.get(&from).is_none());
    }

    #[test]
    fn propagate_self_edge_is_skipped() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let k = key(1, 1, 1);
        sources.add(k, candidate(0, 0, 10));
        sources.adjacency.insert(k, vec![k]);

        assert!(!sources.propagate(&mut budget));
        assert_eq!(sources.get(&k).unwrap().len(), 1);
    }

    #[test]
    fn propagate_multi_hop() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let a = key(1, 1, 1);
        let b = key(1, 1, 2);
        let c = key(1, 1, 3);

        sources.add(a, candidate(0, 0, 10));
        sources.adjacency.insert(a, vec![b]);
        sources.adjacency.insert(b, vec![c]);

        assert!(!sources.propagate(&mut budget));

        assert_eq!(sources.get(&b).unwrap().len(), 1);
        assert!(sources.get(&b).unwrap().contains(&candidate(0, 0, 10)));
        assert_eq!(sources.get(&c).unwrap().len(), 1);
        assert!(sources.get(&c).unwrap().contains(&candidate(0, 0, 10)));
    }

    #[test]
    fn propagate_multi_hop_converges() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let a = key(1, 1, 1);
        let b = key(1, 1, 2);

        sources.add(a, candidate(0, 0, 10));
        sources.adjacency.insert(a, vec![b]);
        sources.adjacency.insert(b, vec![a]);

        let exhausted = sources.propagate(&mut budget);
        assert!(!exhausted);
        assert!(sources.get(&b).unwrap().contains(&candidate(0, 0, 10)));
    }

    #[test]
    fn propagate_preserves_ordering_at_destination() {
        let mut sources = FlowSources::default();
        let mut budget = Budget::new(usize::MAX);
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(to, candidate(0, 0, 5));
        sources.add(from, candidate(0, 1, 20));
        sources.add(from, candidate(0, 0, 10));
        sources.adjacency.insert(from, vec![to]);

        sources.propagate(&mut budget);

        let ordered: Vec<_> = sources.get(&to).unwrap().iter().copied().collect();
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
            sources.add(a, candidate(0, 0, i));
        }
        // a → b edges cause all candidates to flow into b in one round,
        // filling the pending queue past the safety limit.
        sources.adjacency.insert(a, vec![b]);

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

        sources.add(k, candidate(0, 2, 30));
        sources.add(k, candidate(0, 0, 10));
        sources.add(k, candidate(0, 1, 20));

        let ordered: Vec<_> = sources.get(&k).unwrap().iter().copied().collect();
        assert_eq!(ordered[0], candidate(0, 0, 10));
        assert_eq!(ordered[1], candidate(0, 1, 20));
        assert_eq!(ordered[2], candidate(0, 2, 30));
    }

    fn context(module: u32, function: u32) -> CallContext {
        CallContext {
            module: ModuleId::new(module),
            function: FunctionId::from_test(function),
            parameter: None,
            source_root: None,
            state: CrossFlowState {
                flow: FlowId::new(RuleIndex::new(0), 0),
                source: Some(QualifiedEvent {
                    module: ModuleId::new(1),
                    fact: FactId::from_test(1),
                }),
                requirements: RequirementSet::default(),
                sinks: RequirementSet::default(),
            },
            crossed: false,
        }
    }

    #[test]
    fn worklist_len_counts_total_retained_not_pending() {
        let mut wl = ContextWorklist::new(10);
        assert_eq!(wl.len(), 0);

        // Push two contexts
        assert!(wl.push(context(1, 1)));
        assert_eq!(wl.len(), 1);
        assert!(wl.push(context(1, 2)));
        assert_eq!(wl.len(), 2);

        // Pop one — seen still retains both, so len is still 2
        let _popped = wl.pop_front();
        assert_eq!(wl.len(), 2);

        // Duplicate push does not increase retained count
        assert!(!wl.push(context(1, 1)));
        assert_eq!(wl.len(), 2);
    }

    #[test]
    fn worklist_respects_max_retained_limit() {
        let mut wl = ContextWorklist::new(3);
        assert!(wl.push(context(1, 1)));
        assert!(wl.push(context(1, 2)));
        assert!(wl.push(context(1, 3)));
        // Fourth unique context hits the limit
        assert!(!wl.push(context(1, 4)));
        assert_eq!(wl.len(), 3);
        assert!(wl.is_exhausted());
    }

    #[test]
    fn worklist_is_exhausted_false_below_limit() {
        let mut wl = ContextWorklist::new(5);
        assert!(!wl.is_exhausted());
        wl.push(context(1, 1));
        assert!(!wl.is_exhausted());
    }
}
