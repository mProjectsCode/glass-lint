use std::collections::{BTreeMap, BTreeSet, VecDeque};

use glass_lint_datastructures::{Budget, NameTable};
use hashbrown::HashMap;

use crate::{
    analysis::{
        ProjectSemanticModel,
        facts::FactId,
        flow::{
            cross::{MAX_PENDING, QualifiedCallGraph},
            planning::{BoundLifecycleCallTarget, BoundTargetIndex},
        },
        model::flow::FlowId,
        value::{FunctionId, ValueId},
    },
    api::compiler::CompiledObjectFlow,
    project::ModuleId,
};

/// Local effect/value key used while composing source identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct SourceKey {
    pub(super) module: ModuleId,
    pub(super) function: FunctionId,
    pub(super) value: ValueId,
}

impl SourceKey {
    pub(super) fn new(module: ModuleId, function: FunctionId, value: ValueId) -> Self {
        Self {
            module,
            function,
            value,
        }
    }
}

/// Flow matcher and source-event pair associated with a source identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct SourceCandidate {
    pub(super) flow: FlowId,
    pub(super) fact: FactId,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PropagationItem {
    key: SourceKey,
    candidate: SourceCandidate,
}

/// Proven source identities indexed by the local effect that produced them.
///
/// Uses `BTreeSet` per bucket so insertion is deduplicated and sorted by
/// construction; the table never needs a separate normalize pass.
///
/// Propagation uses an adjacency index built from the call graph so that
/// only edges reachable from a changed key are visited, and a candidate-level
/// worklist so that only newly inserted candidates are re-propagated.
#[derive(Default)]
pub(super) struct FlowSources {
    sources: BTreeMap<SourceKey, BTreeSet<SourceCandidate>>,
    adjacency: BTreeMap<SourceKey, Vec<SourceKey>>,
}

impl FlowSources {
    pub(super) fn add_candidate(&mut self, key: SourceKey, candidate: SourceCandidate) -> bool {
        self.sources.entry(key).or_default().insert(candidate)
    }

    pub(super) fn candidates(&self, key: &SourceKey) -> impl Iterator<Item = &SourceCandidate> {
        self.sources.get(key).into_iter().flat_map(BTreeSet::iter)
    }

    #[cfg(test)]
    pub(super) fn has_candidates(&self, key: &SourceKey) -> bool {
        self.sources.contains_key(key)
    }

    #[cfg(test)]
    pub(super) fn candidate_count(&self, key: &SourceKey) -> usize {
        self.candidates(key).count()
    }

    #[cfg(test)]
    pub(super) fn contains_candidate(&self, key: &SourceKey, candidate: SourceCandidate) -> bool {
        self.candidates(key).any(|stored| *stored == candidate)
    }

    pub(super) fn candidate_entries(&self) -> impl Iterator<Item = (SourceKey, SourceCandidate)> {
        self.sources.iter().flat_map(|(key, candidates)| {
            candidates.iter().map(move |candidate| (*key, *candidate))
        })
    }

    pub(super) fn add_edge(&mut self, from: SourceKey, to: SourceKey) {
        let destinations = self.adjacency.entry(from).or_default();
        match destinations.binary_search(&to) {
            Ok(_) => {}
            Err(position) => destinations.insert(position, to),
        }
    }

    pub(super) fn destinations(&self, key: &SourceKey) -> impl Iterator<Item = &SourceKey> {
        self.adjacency
            .get(key)
            .into_iter()
            .flat_map(|destinations| destinations.iter())
    }

    /// Build the adjacency index in one pass over all modules, effects, and
    /// calls.  Each edge records that the destination key should receive
    /// candidates from the source key when the source key changes.
    fn build_adjacency(&mut self, project: &ProjectSemanticModel, call_graph: &QualifiedCallGraph) {
        for module in project.modules() {
            let stream = module.local().facts().stream();
            for effect in module.local().effects().iter_effects() {
                if effect.is_invalid() {
                    continue;
                }
                for call in effect.calls() {
                    let cref = call.as_ref(stream);
                    let Some((target_module, target_function)) =
                        call_graph.get(module.id(), call.event())
                    else {
                        continue;
                    };
                    let Some(target) = project.effect(target_module, target_function) else {
                        continue;
                    };

                    let to = SourceKey::new(module.id(), effect.id(), cref.result());

                    for returned in target.returns().iter().filter(|r| r.parameter().is_none()) {
                        let root = target
                            .value_root(returned.value())
                            .unwrap_or_else(|| returned.value());
                        let from = SourceKey::new(target_module, target_function, root);
                        self.add_edge(from, to);
                    }

                    for argument in call.arguments() {
                        if !argument.is_root()
                            || !target.returns().iter().any(|returned| {
                                returned.parameter().is_some_and(|parameter| {
                                    parameter.index() == argument.index() && parameter.is_root()
                                })
                            })
                        {
                            continue;
                        }
                        let root = effect
                            .value_root(argument.value())
                            .unwrap_or_else(|| argument.value());
                        let from = SourceKey::new(module.id(), effect.id(), root);
                        self.add_edge(from, to);
                    }
                }
            }
        }
    }
}

type SourceIndex = BoundTargetIndex<FlowId>;

/// Build a per-module source index mapping typed call targets to flow IDs.
fn build_source_index(
    flows: &HashMap<FlowId, &CompiledObjectFlow>,
    names: &NameTable,
) -> SourceIndex {
    let mut index = SourceIndex::default();
    for (id, flow) in flows {
        for source in &flow.sources {
            if let Some(target) = BoundLifecycleCallTarget::from_lifecycle(&source.target, names) {
                index.insert(target, *id);
            }
        }
    }
    index.normalize();
    index
}

impl FlowSources {
    pub(super) fn collect(
        project: &ProjectSemanticModel,
        flows: &HashMap<FlowId, &CompiledObjectFlow>,
        call_graph: &QualifiedCallGraph,
        budget: &mut Budget,
    ) -> (Self, bool) {
        let mut sources = Self::default();
        sources.collect_candidates(project, flows);
        sources.build_adjacency(project, call_graph);
        let budget_exhausted = sources.propagate(budget);
        (sources, budget_exhausted)
    }

    fn collect_candidates(
        &mut self,
        project: &ProjectSemanticModel,
        flows: &HashMap<FlowId, &CompiledObjectFlow>,
    ) {
        for module in project.modules() {
            let names = module.local().facts().names();
            let stream = module.local().facts().stream();
            // Build a per-module source index so that candidate discovery
            // looks up flows by chain instead of scanning every flow for
            // every call.
            let source_index = build_source_index(flows, names);
            for effect in module.local().effects().iter_effects() {
                if effect.is_invalid() {
                    continue;
                }
                for call in effect.calls() {
                    let cref = call.as_ref(stream);
                    let candidates = cref
                        .global_name()
                        .and_then(|name| {
                            source_index.get(&BoundLifecycleCallTarget::Global(name.clone()))
                        })
                        .or_else(|| {
                            cref.chain().and_then(|chain| {
                                source_index.get(&BoundLifecycleCallTarget::Member(chain.clone()))
                            })
                        });
                    let Some(candidates) = candidates else {
                        continue;
                    };
                    for flow_id in candidates {
                        let Some(flow) = flows.get(flow_id) else {
                            continue;
                        };
                        if cref.matches_source(flow, names) {
                            self.add_candidate(
                                SourceKey::new(module.id(), effect.id(), cref.result()),
                                SourceCandidate {
                                    flow: *flow_id,
                                    fact: call.event(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    /// Propagate source candidates through the pre-built adjacency index using
    /// a candidate-level worklist.  Each round dequeues the pending batch and
    /// inserts each candidate into every destination key reachable from its
    /// source.  Destinations that receive a new candidate are enqueued for the
    /// next round, forming a monotone fixed-point iteration over the call-graph
    /// edges without re-scanning the project.
    ///
    /// Both the pending frontier and the total unique seen-set are bounded so
    /// that a long, narrow propagation graph cannot retain unbounded state
    /// without tripping the frontier limit.
    pub(super) fn propagate(&mut self, budget: &mut Budget) -> bool {
        let mut pending = VecDeque::<PropagationItem>::new();
        let mut pending_seen = BTreeSet::<PropagationItem>::new();

        for (key, candidate) in self.candidate_entries() {
            if pending_seen.len() >= MAX_PENDING {
                return true;
            }
            let entry = PropagationItem { key, candidate };
            if pending_seen.insert(entry) {
                pending.push_back(entry);
            }
        }

        while !pending.is_empty() {
            let round = std::mem::take(&mut pending);

            for item in &round {
                let destinations: Vec<_> = self.destinations(&item.key).copied().collect();
                for to_key in destinations {
                    if to_key == item.key {
                        continue;
                    }
                    if self.add_candidate(to_key, item.candidate) {
                        if !budget.try_push() {
                            return true;
                        }
                        if pending_seen.len() >= MAX_PENDING {
                            return true;
                        }
                        let entry = PropagationItem {
                            key: to_key,
                            candidate: item.candidate,
                        };
                        if pending_seen.insert(entry) {
                            pending.push_back(entry);
                        }
                    }
                }
            }

            if pending.len() >= MAX_PENDING {
                return true;
            }
        }

        budget.exhausted()
    }
}
