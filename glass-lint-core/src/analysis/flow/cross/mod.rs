//! Project-level flow projection over qualified function effects.
//!
//! This pass is deliberately small and bounded. Local object flow remains the
//! source of truth for one module; this overlay carries a proven object state
//! through parameter-to-call relations and qualified call edges.

mod propagation;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use glass_lint_datastructures::{Budget, NamePath, NameTable};

use crate::{
    analysis::{
        ProjectSemanticModel,
        facts::FactId,
        flow::{
            effect::{EffectUse, FunctionEffect},
            index::FlowId,
            requirements::RequirementSet,
        },
        value::{FunctionId, ValueId},
    },
    api::{
        classification::{ClassificationEvidence, MatchKind, RelatedClassificationEvidence},
        compiler::{CompiledObjectFlow, CompiledObjectRequirement, CompiledRuleSelection},
    },
    project::ModuleId,
};

const MAX_CONTEXTS: usize = 65_536;
const MAX_SOURCE_REFINEMENT_ROUNDS: usize = 64;
const MAX_PENDING: usize = 65_536;
const MAX_RELATED_EVIDENCE: usize = 8;

/// Pre-computed qualified call targets keyed by (caller_module,
/// call_event_fact). Populated once and reused across all cross-flow phases.
pub(super) struct QualifiedCallGraph {
    targets: BTreeMap<(ModuleId, FactId), (ModuleId, FunctionId)>,
}

impl QualifiedCallGraph {
    fn build(project: &ProjectSemanticModel) -> Self {
        let mut targets = BTreeMap::new();
        for module in project.modules() {
            let module_id = module.id();
            let stream = module.local().facts().stream();
            for effect in module.local().effects().iter_effects() {
                for call in effect.calls() {
                    let cref = call.as_ref(stream);
                    let Some(provenance) = cref.provenance() else {
                        continue;
                    };
                    if let Some(target) =
                        project.qualified_function_target(module_id, cref.target(), provenance)
                    {
                        targets.insert((module_id, call.event()), target);
                    }
                }
            }
        }
        Self { targets }
    }

    pub(super) fn get(&self, module: ModuleId, event: FactId) -> Option<(ModuleId, FunctionId)> {
        self.targets.get(&(module, event)).copied()
    }
}

/// Pre-resolved requirement and sink member paths for one (flow, names) pair.
/// Built once and reused across all contexts for the same flow and module.
pub(super) struct FlowPathPlan {
    pub(super) req_members: Vec<Option<NamePath>>,
    pub(super) sink_members: Vec<Vec<NamePath>>,
}

impl FlowPathPlan {
    pub(super) fn build(flow: &CompiledObjectFlow, names: &NameTable) -> Self {
        let req_members = flow
            .requirements
            .iter()
            .map(|req| match req {
                CompiledObjectRequirement::MemberCall { member, .. } => names.lookup_path(member),
                CompiledObjectRequirement::PropertyWrite { .. } => None,
            })
            .collect();
        let sink_members = flow
            .sinks
            .iter()
            .map(|sink| {
                sink.member_calls
                    .iter()
                    .filter_map(|mc| names.lookup_path(mc))
                    .collect()
            })
            .collect();
        Self {
            req_members,
            sink_members,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum EvidenceRole {
    Source,
    Requirement,
    Sink,
}

impl EvidenceRole {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Source => "flow source",
            Self::Requirement => "flow requirement",
            Self::Sink => "flow sink",
        }
    }
}

#[derive(Debug)]
/// Fixed-point budget for propagating source identities through helper calls.
struct SourceBudget {
    rounds: usize,
}

impl SourceBudget {
    fn new() -> Self {
        Self { rounds: 0 }
    }

    fn next_round(&mut self) -> bool {
        self.rounds = self.rounds.saturating_add(1);
        self.rounds <= MAX_SOURCE_REFINEMENT_ROUNDS
    }

    fn exhausted(&self) -> bool {
        self.rounds > MAX_SOURCE_REFINEMENT_ROUNDS
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
/// A fact location qualified by its owning project module.
pub(super) struct QualifiedEvent {
    pub(super) module: ModuleId,
    pub(super) fact: FactId,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
/// Monotone flow state carried through one qualified call context.
pub(super) struct CrossFlowState {
    pub(super) flow: FlowId,
    pub(super) source: QualifiedEvent,
    pub(super) requirements: RequirementSet<QualifiedEvent>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
/// Worklist context identifying the function/value path currently projected.
pub(super) struct CallContext {
    pub(super) module: ModuleId,
    pub(super) function: FunctionId,
    pub(super) parameter: Option<usize>,
    pub(super) source_root: Option<ValueId>,
    pub(super) state: CrossFlowState,
    pub(super) crossed: bool,
}

/// Deduplicating FIFO worklist for bounded interprocedural contexts.
///
/// Uses `VecDeque` for O(1) pop-front and a `BTreeSet` for O(log n) dedup,
/// avoiding the O(n) shift cost of `IndexSet::shift_remove_index(0)`.
///
/// The worklist enforces [`MAX_CONTEXTS`] total retained contexts so that
/// the seen-set (not only the pending frontier) is bounded.
struct ContextWorklist {
    /// FIFO queue of pending contexts.
    queue: VecDeque<CallContext>,
    /// Seen-set for O(log n) deduplication and total-retained tracking.
    seen: BTreeSet<CallContext>,
    /// Maximum unique contexts retained before dropping new ones.
    max_retained: usize,
}

impl ContextWorklist {
    pub(super) fn new(max_retained: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            seen: BTreeSet::new(),
            max_retained,
        }
    }

    /// Push a context if the total-retained budget allows.
    ///
    /// Returns whether the context was newly added.  Contexts beyond the
    /// retained limit are silently dropped (the caller will detect
    /// exhaustion via [`len`] / [`is_exhausted`]).
    pub(super) fn push(&mut self, context: CallContext) -> bool {
        if self.seen.len() >= self.max_retained {
            return false;
        }
        if self.seen.insert(context.clone()) {
            self.queue.push_back(context);
            true
        } else {
            false
        }
    }

    pub(super) fn pop_front(&mut self) -> Option<CallContext> {
        let context = self.queue.pop_front()?;
        Some(context)
    }

    /// Total unique contexts retained (seen-set size).
    ///
    /// This is the meaningful bound for worklist memory: it counts every
    /// unique context ever inserted, not only the pending frontier.
    pub(super) fn len(&self) -> usize {
        self.seen.len()
    }

    pub(super) fn is_exhausted(&self) -> bool {
        self.seen.len() >= self.max_retained
    }

    pub(super) fn enqueue_parameters(
        &mut self,
        project: &ProjectSemanticModel,
        module: ModuleId,
        function: FunctionId,
        argument_index: usize,
        state: &CrossFlowState,
        crossed: bool,
    ) {
        let Some(effect) = project.effect(module, function) else {
            return;
        };
        let Some(fact_stream) = project.module_fact_stream(module) else {
            return;
        };
        for parameter in effect.parameters(fact_stream).iter().filter(|parameter| {
            parameter.parameter_index == argument_index && parameter.path.is_empty()
        }) {
            if self.is_exhausted() {
                return;
            }
            self.push(CallContext {
                module,
                function,
                parameter: Some(parameter.parameter_index),
                source_root: None,
                state: state.clone(),
                crossed,
            });
        }
    }

    fn seed(
        project: &ProjectSemanticModel,
        sources: &FlowSources,
        call_graph: &QualifiedCallGraph,
    ) -> Self {
        let mut worklist = Self::new(MAX_CONTEXTS);
        worklist.seed_from_sources(project, sources);
        worklist.seed_from_calls(project, sources, call_graph);
        worklist
    }

    fn seed_from_sources(&mut self, project: &ProjectSemanticModel, sources: &FlowSources) {
        for (key, candidates) in sources.iter() {
            if self.is_exhausted() {
                return;
            }
            for candidate in candidates {
                self.push(CallContext {
                    module: key.module,
                    function: key.function,
                    parameter: None,
                    source_root: Some(key.value),
                    state: CrossFlowState {
                        flow: candidate.flow,
                        source: QualifiedEvent {
                            module: key.module,
                            fact: candidate.fact,
                        },
                        requirements: RequirementSet::default(),
                    },
                    crossed: key.value != project.source_call_result(key.module, candidate.fact),
                });
            }
        }
    }

    fn seed_from_calls(
        &mut self,
        project: &ProjectSemanticModel,
        sources: &FlowSources,
        call_graph: &QualifiedCallGraph,
    ) {
        for module in project.modules() {
            if self.is_exhausted() {
                return;
            }
            for effect in module.local().effects().iter_effects() {
                for call in effect.calls() {
                    let Some((target_module, target_function)) =
                        call_graph.get(module.id(), call.event())
                    else {
                        continue;
                    };
                    for argument in call.arguments() {
                        if !argument.is_root() {
                            continue;
                        }
                        let root = effect
                            .value_root(argument.value())
                            .unwrap_or_else(|| argument.value());
                        let Some(candidates) =
                            sources.get(&SourceKey::new(module.id(), effect.id(), root))
                        else {
                            continue;
                        };
                        for candidate in candidates {
                            let state = CrossFlowState {
                                flow: candidate.flow,
                                source: QualifiedEvent {
                                    module: module.id(),
                                    fact: candidate.fact,
                                },
                                requirements: RequirementSet::default(),
                            };
                            self.enqueue_parameters(
                                project,
                                target_module,
                                target_function,
                                argument.index(),
                                &state,
                                target_module != module.id(),
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Local effect/value key used while composing source identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SourceKey {
    module: ModuleId,
    function: FunctionId,
    value: ValueId,
}

impl SourceKey {
    fn new(module: ModuleId, function: FunctionId, value: ValueId) -> Self {
        Self {
            module,
            function,
            value,
        }
    }
}

/// Flow matcher and source-event pair associated with a source identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SourceCandidate {
    flow: FlowId,
    fact: FactId,
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
struct FlowSources {
    sources: BTreeMap<SourceKey, BTreeSet<SourceCandidate>>,
    adjacency: BTreeMap<SourceKey, Vec<SourceKey>>,
}

impl FlowSources {
    fn add(&mut self, key: SourceKey, candidate: SourceCandidate) {
        self.sources.entry(key).or_default().insert(candidate);
    }

    fn get(&self, key: &SourceKey) -> Option<&BTreeSet<SourceCandidate>> {
        self.sources.get(key)
    }

    fn iter(&self) -> impl Iterator<Item = (&SourceKey, &BTreeSet<SourceCandidate>)> {
        self.sources.iter()
    }

    /// Build the adjacency index in one pass over all modules, effects, and
    /// calls.  Each edge records that the destination key should receive
    /// candidates from the source key when the source key changes.
    fn build_adjacency(&mut self, project: &ProjectSemanticModel, call_graph: &QualifiedCallGraph) {
        for module in project.modules() {
            let stream = module.local().facts().stream();
            for effect in module.local().effects().iter_effects() {
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
                        self.adjacency.entry(from).or_default().push(to);
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
                        self.adjacency.entry(from).or_default().push(to);
                    }
                }
            }
        }
    }
}

/// Build a per-module source index mapping NamePath to matching flow IDs.
fn build_source_index(
    flows: &BTreeMap<FlowId, &CompiledObjectFlow>,
    names: &glass_lint_datastructures::NameTable,
) -> BTreeMap<glass_lint_datastructures::NamePath, Vec<FlowId>> {
    let mut index: BTreeMap<glass_lint_datastructures::NamePath, Vec<FlowId>> = BTreeMap::new();
    for (id, flow) in flows {
        for source in &flow.sources {
            if let Some(member) = names.lookup_path(&source.member_call) {
                index.entry(member).or_default().push(*id);
            }
        }
    }
    for ids in index.values_mut() {
        ids.sort_unstable();
        ids.dedup();
    }
    index
}

pub(in crate::analysis) fn collect(
    project: &ProjectSemanticModel,
    matchers: &CompiledRuleSelection<'_>,
) -> (
    BTreeMap<ModuleId, Vec<Vec<ClassificationEvidence>>>,
    bool,
    usize,
) {
    let mut flows = BTreeMap::<FlowId, &CompiledObjectFlow>::new();
    for (rule_index, matcher) in matchers.selected_matchers() {
        for (flow_index, flow) in matcher.query().flows().iter().enumerate() {
            flows.insert(FlowId::new(rule_index, flow_index), flow);
        }
    }
    let rule_count = matchers.len();
    let mut evidence = project
        .modules()
        .map(|module| (module.id(), ModuleEvidence::new(rule_count)))
        .collect::<BTreeMap<_, _>>();
    if flows.is_empty() {
        let empty = evidence
            .into_iter()
            .map(|(id, m)| (id, m.evidence))
            .collect();
        return (empty, false, 0);
    }

    let call_graph = QualifiedCallGraph::build(project);
    let (sources, return_budget_exhausted) = FlowSources::collect(project, &flows, &call_graph);
    let mut worklist = ContextWorklist::seed(project, &sources, &call_graph);

    let mut flow_plans: BTreeMap<(FlowId, ModuleId), FlowPathPlan> = BTreeMap::new();
    for (flow_id, flow) in &flows {
        for module in project.modules() {
            let Some(names) = project.module_names(module.id()) else {
                continue;
            };
            flow_plans.insert((*flow_id, module.id()), FlowPathPlan::build(flow, names));
        }
    }

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
        let Some(flow_plan) = flow_plans.get(&(context.state.flow, context.module)) else {
            continue;
        };
        let mut current_state = context.state.clone();
        let mut propagated_calls = BTreeSet::new();
        let names = project
            .module_names(context.module)
            .expect("module has names");
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

impl FlowSources {
    fn collect(
        project: &ProjectSemanticModel,
        flows: &BTreeMap<FlowId, &CompiledObjectFlow>,
        call_graph: &QualifiedCallGraph,
    ) -> (Self, bool) {
        let mut sources = Self::default();
        sources.collect_candidates(project, flows);
        sources.build_adjacency(project, call_graph);
        let budget_exhausted = sources.propagate();
        (sources, budget_exhausted)
    }

    fn collect_candidates(
        &mut self,
        project: &ProjectSemanticModel,
        flows: &BTreeMap<FlowId, &CompiledObjectFlow>,
    ) {
        for module in project.modules() {
            let names = module.local().facts().names();
            let stream = module.local().facts().stream();
            // Build a per-module source index so that candidate discovery
            // looks up flows by chain instead of scanning every flow for
            // every call.
            let source_index = build_source_index(flows, names);
            for effect in module.local().effects().iter_effects() {
                for call in effect.calls() {
                    let cref = call.as_ref(stream);
                    let Some(chain) = cref.chain() else {
                        continue;
                    };
                    let Some(candidates) = source_index.get(chain) else {
                        continue;
                    };
                    for flow_id in candidates {
                        let Some(flow) = flows.get(flow_id) else {
                            continue;
                        };
                        if cref.matches_source(flow, names) {
                            self.add(
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
    fn propagate(&mut self) -> bool {
        let mut budget = SourceBudget::new();

        let mut pending: VecDeque<(SourceKey, SourceCandidate)> = VecDeque::new();
        let mut pending_seen: BTreeSet<(SourceKey, SourceCandidate)> = BTreeSet::new();

        for (key, candidates) in &self.sources {
            for &candidate in candidates {
                if pending_seen.len() >= MAX_PENDING {
                    return true;
                }
                let entry = (*key, candidate);
                if pending_seen.insert(entry) {
                    pending.push_back(entry);
                }
            }
        }

        while !pending.is_empty() && budget.next_round() {
            let round = std::mem::take(&mut pending);

            for (from_key, candidate) in &round {
                let Some(destinations) = self.adjacency.get(from_key) else {
                    continue;
                };
                for &to_key in destinations {
                    if to_key == *from_key {
                        continue;
                    }
                    if self.sources.entry(to_key).or_default().insert(*candidate) {
                        if pending_seen.len() >= MAX_PENDING {
                            return true;
                        }
                        let entry = (to_key, *candidate);
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

pub(super) fn effect_use_event(usage: &EffectUse) -> FactId {
    match usage {
        EffectUse::PropertyWrite { event, .. }
        | EffectUse::CallArgument { event, .. }
        | EffectUse::CallReceiver { event, .. } => *event,
    }
}

pub(super) fn usage_matches_context(
    effect: &FunctionEffect,
    usage: &EffectUse,
    context: &CallContext,
) -> bool {
    match usage {
        EffectUse::PropertyWrite {
            receiver,
            receiver_value,
            ..
        } => {
            receiver.as_ref().is_some_and(|parameter| {
                context
                    .parameter
                    .is_some_and(|index| parameter.index() == index && parameter.is_root())
            }) || (context.parameter.is_none()
                && context.source_root.is_some_and(|root| {
                    effect
                        .value_root(*receiver_value)
                        .unwrap_or(*receiver_value)
                        == root
                }))
        }
        EffectUse::CallReceiver { receiver, .. } => context
            .parameter
            .is_some_and(|index| receiver.index() == index && receiver.is_root()),
        EffectUse::CallArgument {
            call_id,
            argument_index,
            ..
        } => effect
            .call_argument(*call_id, *argument_index)
            .is_some_and(|argument| {
                argument.parameter().is_some_and(|parameter| {
                    context
                        .parameter
                        .is_some_and(|index| parameter.index() == index && parameter.is_root())
                }) || (context.parameter.is_none()
                    && context.source_root.is_some_and(|root| {
                        effect
                            .value_root(argument.value())
                            .unwrap_or_else(|| argument.value())
                            == root
                    }))
            }),
    }
}

pub(super) struct ModuleEvidence {
    pub(super) evidence: Vec<Vec<ClassificationEvidence>>,
    pub(super) seen: Vec<BTreeSet<(MatchKind, String, u32)>>,
}

impl ModuleEvidence {
    pub(super) fn new(rule_count: usize) -> Self {
        Self {
            evidence: vec![Vec::new(); rule_count],
            seen: vec![BTreeSet::new(); rule_count],
        }
    }
}

pub(super) fn emit(
    project: &ProjectSemanticModel,
    evidence: &mut BTreeMap<ModuleId, ModuleEvidence>,
    module: ModuleId,
    flow_id: FlowId,
    state: &CrossFlowState,
    event: FactId,
    flow: &CompiledObjectFlow,
) {
    let Some(values) = evidence.get_mut(&module) else {
        return;
    };
    let rule_idx = flow_id.rule_index().get();
    if !values.seen[rule_idx].insert((MatchKind::CallArgument, flow.symbol.clone(), event.0)) {
        return;
    }
    let span = project
        .fact(module, event)
        .map_or_else(glass_lint_datastructures::ByteRange::empty, |fact| {
            fact.span
        });
    values.evidence[rule_idx].push(ClassificationEvidence {
        kind: MatchKind::CallArgument,
        symbol: flow.evidence_symbol(),
        count: 1,
        truncated: false,
        occurrences: vec![
            crate::api::classification::ClassificationEvidenceOccurrence {
                span,
                fact: Some(event.0),
            },
        ],
        related: related_evidence(state, module, event),
    });
}

pub(super) fn related_evidence(
    state: &CrossFlowState,
    sink_module: ModuleId,
    sink_event: FactId,
) -> Vec<RelatedClassificationEvidence> {
    let mut related = vec![related_event(&state.source, EvidenceRole::Source)];
    related.extend(
        state
            .requirements
            .values()
            .map(|event| related_event(event, EvidenceRole::Requirement)),
    );
    related.push(RelatedClassificationEvidence {
        module: sink_module.get(),
        event: sink_event.0,
        kind: MatchKind::CallArgument,
        symbol: EvidenceRole::Sink.label().into(),
    });
    let mut seen = BTreeSet::new();
    related.retain(|item| seen.insert((item.module, item.event, item.kind, item.symbol.clone())));
    related.truncate(MAX_RELATED_EVIDENCE);
    related
}

pub(super) fn related_event(
    event: &QualifiedEvent,
    role: EvidenceRole,
) -> RelatedClassificationEvidence {
    RelatedClassificationEvidence {
        module: event.module.get(),
        event: event.fact.0,
        kind: MatchKind::CallArgument,
        symbol: role.label().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::classification::RuleIndex;

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
