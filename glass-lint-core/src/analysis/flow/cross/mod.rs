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
        flow::cross::{
            evidence::ModuleEvidence,
            graph::{FlowPathPlan, QualifiedCallGraph},
            sources::FlowSources,
            worklist::ContextWorklist,
        },
        model::flow::FlowId,
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
const MAX_SOURCE_REFINEMENT_ROUNDS: usize = 64;
const MAX_PENDING: usize = 65_536;
pub(in crate::analysis) fn collect(
    project: &ProjectSemanticModel,
    matchers: &CompiledRuleSelection<'_>,
    session: &mut LinkingSession,
    arena: &mut TraceArena,
) -> (
    BTreeMap<ModuleId, Vec<Vec<ClassificationEvidence>>>,
    bool,
    usize,
) {
    // Single worklist loop: setup, iteration with UsageProjector and
    // CallPropagation per context, then final exhaustion handling.
    // Extracting the loop body would require passing 12+ context fields
    // through every call site.
    let mut flows = HashMap::<FlowId, &CompiledObjectFlow>::new();
    for (rule_index, matcher) in matchers.selected_matchers() {
        for (flow_index, flow) in matcher.query().flows().iter().enumerate() {
            flows.insert(FlowId::new(rule_index, flow_index), flow);
        }
    }
    let rule_count = matchers.len();
    let mut evidence = project
        .modules()
        .map(|module| (module.id(), ModuleEvidence::new(rule_count)))
        .collect::<HashMap<_, _>>();
    if flows.is_empty() {
        let empty = evidence
            .into_iter()
            .map(|(id, m)| (id, m.evidence))
            .collect();
        return (empty, false, 0);
    }

    let call_graph = QualifiedCallGraph::build(project, session);
    let (sources, return_budget_exhausted) = FlowSources::collect(project, &flows, &call_graph);
    let mut worklist = ContextWorklist::seed(project, &sources, &call_graph);

    let mut flow_plan_cache: HashMap<(FlowId, ModuleId), FlowPathPlan> = HashMap::new();

    let mut step_budget = Budget::new(project.flow_limit());
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
        let mut current_state = context.state.clone();
        let mut propagated_calls = BTreeSet::new();
        propagation::UsageProjector {
            project,
            evidence: &mut evidence,
            context: &context,
            effect,
            flow,
            flow_plan,
            call_graph: &call_graph,
            state: &mut current_state,
            propagated: &mut propagated_calls,
            worklist: &mut worklist,
            names,
            arena,
        }
        .project();
        propagation::CallPropagation {
            project,
            effect,
            module: context.module,
            context: &context,
            propagated: &mut propagated_calls,
            through: None,
            state: &current_state,
            worklist: &mut worklist,
            call_graph: &call_graph,
        }
        .propagate();
        if worklist.len() >= MAX_CONTEXTS {
            break;
        }
    }
    let exhausted =
        return_budget_exhausted || step_budget.exhausted() || worklist.len() >= MAX_CONTEXTS;
    if exhausted {
        for module_evidence in evidence.values_mut() {
            for rule in &mut module_evidence.evidence {
                rule.clear();
            }
        }
    }
    let output = evidence
        .into_iter()
        .map(|(id, m)| (id, m.evidence))
        .collect();
    (output, exhausted, projections)
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
        SourceKey::new(ModuleId::new(module), FunctionId(function), ValueId(value))
    }

    fn candidate(rule: usize, flow: usize, fact: u32) -> SourceCandidate {
        SourceCandidate {
            flow: FlowId::new(RuleIndex::new(rule), flow),
            fact: FactId(fact),
        }
    }

    #[test]
    fn propagate_transfers_along_adjacency_edge() {
        let mut sources = FlowSources::default();
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(from, candidate(0, 0, 10));
        sources.add(from, candidate(0, 0, 20));
        sources.adjacency.insert(from, vec![to]);

        assert!(!sources.propagate());

        let dest = sources.get(&to).unwrap();
        assert_eq!(dest.len(), 2);
        assert!(dest.contains(&candidate(0, 0, 10)));
        assert!(dest.contains(&candidate(0, 0, 20)));
    }

    #[test]
    fn propagate_deduplicates_by_construction() {
        let mut sources = FlowSources::default();
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(from, candidate(0, 0, 10));
        sources.adjacency.insert(from, vec![to]);

        assert!(!sources.propagate());
        assert_eq!(sources.get(&to).unwrap().len(), 1);

        // Second propagation is a no-op because candidates are already at the
        // destination.
        assert!(!sources.propagate());
        assert_eq!(sources.get(&to).unwrap().len(), 1);
    }

    #[test]
    fn propagate_partial_novelty() {
        let mut sources = FlowSources::default();
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(from, candidate(0, 0, 10));
        sources.add(from, candidate(0, 0, 20));
        sources.add(to, candidate(0, 0, 10));
        sources.adjacency.insert(from, vec![to]);

        assert!(!sources.propagate());
        assert_eq!(sources.get(&to).unwrap().len(), 2);

        assert!(!sources.propagate());
    }

    #[test]
    fn propagate_missing_source_is_no_op() {
        let mut sources = FlowSources::default();
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.adjacency.insert(from, vec![to]);

        assert!(!sources.propagate());
        assert!(sources.get(&to).is_none());
        assert!(sources.get(&from).is_none());
    }

    #[test]
    fn propagate_self_edge_is_skipped() {
        let mut sources = FlowSources::default();
        let k = key(1, 1, 1);
        sources.add(k, candidate(0, 0, 10));
        sources.adjacency.insert(k, vec![k]);

        assert!(!sources.propagate());
        assert_eq!(sources.get(&k).unwrap().len(), 1);
    }

    #[test]
    fn propagate_multi_hop() {
        let mut sources = FlowSources::default();
        let a = key(1, 1, 1);
        let b = key(1, 1, 2);
        let c = key(1, 1, 3);

        sources.add(a, candidate(0, 0, 10));
        sources.adjacency.insert(a, vec![b]);
        sources.adjacency.insert(b, vec![c]);

        assert!(!sources.propagate());

        assert_eq!(sources.get(&b).unwrap().len(), 1);
        assert!(sources.get(&b).unwrap().contains(&candidate(0, 0, 10)));
        assert_eq!(sources.get(&c).unwrap().len(), 1);
        assert!(sources.get(&c).unwrap().contains(&candidate(0, 0, 10)));
    }

    #[test]
    fn propagate_multi_hop_converges() {
        let mut sources = FlowSources::default();
        let a = key(1, 1, 1);
        let b = key(1, 1, 2);

        sources.add(a, candidate(0, 0, 10));
        sources.adjacency.insert(a, vec![b]);
        sources.adjacency.insert(b, vec![a]);

        let exhausted = sources.propagate();
        assert!(!exhausted);
        assert!(sources.get(&b).unwrap().contains(&candidate(0, 0, 10)));
    }

    #[test]
    fn propagate_preserves_ordering_at_destination() {
        let mut sources = FlowSources::default();
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(to, candidate(0, 0, 5));
        sources.add(from, candidate(0, 1, 20));
        sources.add(from, candidate(0, 0, 10));
        sources.adjacency.insert(from, vec![to]);

        sources.propagate();

        let ordered: Vec<_> = sources.get(&to).unwrap().iter().copied().collect();
        assert_eq!(ordered[0], candidate(0, 0, 5));
        assert_eq!(ordered[1], candidate(0, 0, 10));
        assert_eq!(ordered[2], candidate(0, 1, 20));
    }

    #[test]
    fn propagate_pending_limit_exhausted() {
        let mut sources = FlowSources::default();
        let a = key(1, 1, 1);
        let b = key(1, 1, 2);
        for i in 0..(u32::try_from(MAX_PENDING).unwrap_or(u32::MAX) + 10) {
            sources.add(a, candidate(0, 0, i));
        }
        // a → b edges cause all candidates to flow into b in one round,
        // filling the pending queue past the safety limit.
        sources.adjacency.insert(a, vec![b]);

        assert!(sources.propagate());
    }

    #[test]
    fn source_budget_round_limit_is_detected() {
        let mut budget = SourceBudget::new();
        for _ in 0..MAX_SOURCE_REFINEMENT_ROUNDS {
            assert!(budget.next_round());
            assert!(!budget.exhausted());
        }
        assert!(!budget.next_round());
        assert!(budget.exhausted());
    }

    #[test]
    fn source_budget_not_exhausted_after_stabilization() {
        let mut budget = SourceBudget::new();
        assert!(budget.next_round());
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
            function: FunctionId(function),
            parameter: None,
            source_root: None,
            state: CrossFlowState {
                flow: FlowId::new(RuleIndex::new(0), 0),
                source: QualifiedEvent {
                    module: ModuleId::new(1),
                    fact: FactId(1),
                },
                requirements: RequirementSet::default(),
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
