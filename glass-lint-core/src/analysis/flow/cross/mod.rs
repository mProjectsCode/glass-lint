//! Project-level flow projection over qualified function effects.
//!
//! This pass is deliberately small and bounded. Local object flow remains the
//! source of truth for one module; this overlay carries a proven object state
//! through parameter-to-call relations and qualified call edges.

mod propagation;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
        compiler::{CompiledObjectFlow, CompiledRuleSelection},
    },
    budget::Budget,
    project::ModuleId,
};

const MAX_CONTEXTS: usize = 65_536;
const MAX_SOURCE_REFINEMENT_ROUNDS: usize = 64;
const MAX_RELATED_EVIDENCE: usize = 8;

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
#[derive(Default)]
struct ContextWorklist {
    /// FIFO queue of pending contexts.
    queue: VecDeque<CallContext>,
    /// Seen-set for O(log n) deduplication.
    seen: BTreeSet<CallContext>,
}

impl ContextWorklist {
    pub(super) fn push(&mut self, context: CallContext) {
        if self.seen.insert(context.clone()) {
            self.queue.push_back(context);
        }
    }

    pub(super) fn pop_front(&mut self) -> Option<CallContext> {
        let context = self.queue.pop_front()?;
        self.seen.remove(&context);
        Some(context)
    }

    pub(super) fn len(&self) -> usize {
        self.queue.len()
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
        for parameter in effect.parameters().iter().filter(|parameter| {
            parameter.parameter_index == argument_index && parameter.path.is_empty()
        }) {
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

    fn seed(project: &ProjectSemanticModel, sources: &FlowSources) -> Self {
        let mut worklist = Self::default();
        worklist.seed_from_sources(project, sources);
        worklist.seed_from_calls(project, sources);
        worklist
    }

    fn seed_from_sources(&mut self, project: &ProjectSemanticModel, sources: &FlowSources) {
        for (key, candidates) in sources.iter() {
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

    fn seed_from_calls(&mut self, project: &ProjectSemanticModel, sources: &FlowSources) {
        for module in project.modules() {
            let stream = module.local().facts().stream();
            for effect in module.local().effects().iter_effects() {
                for call in effect.calls() {
                    let cref = call.as_ref(stream);
                    let Some((target_module, target_function)) = project.qualified_function_target(
                        module.id(),
                        cref.target(),
                        cref.provenance(),
                    ) else {
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
/// construction; the table never needs a separate normalize pass.  Callers that
/// extend from another key collect into a small temporary because the borrow
/// checker cannot split two BTreeMap keys even when they are distinct.
#[derive(Default)]
struct FlowSources(BTreeMap<SourceKey, BTreeSet<SourceCandidate>>);

impl FlowSources {
    fn add(&mut self, key: SourceKey, candidate: SourceCandidate) {
        self.0.entry(key).or_default().insert(candidate);
    }

    fn get(&self, key: &SourceKey) -> Option<&BTreeSet<SourceCandidate>> {
        self.0.get(key)
    }

    fn iter(&self) -> impl Iterator<Item = (&SourceKey, &BTreeSet<SourceCandidate>)> {
        self.0.iter()
    }

    /// Copy candidates from `from` to `to`.  `SourceCandidate` is `Copy`, so
    /// the temporary `Vec` is allocation-cheap and avoids a double borrow on
    /// `self.0` (the get borrows immutably, the entry borrows mutably).
    /// Returns whether new candidates were added at `to`.
    fn extend_from_key(&mut self, to: SourceKey, from: &SourceKey) -> bool {
        if to == *from {
            return false;
        }
        let Some(candidates) = self
            .0
            .get(from)
            .map(|set| set.iter().copied().collect::<Vec<_>>())
        else {
            return false;
        };
        let entry = self.0.entry(to).or_default();
        let mut changed = false;
        for candidate in candidates {
            changed |= entry.insert(candidate);
        }
        changed
    }
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
    let mut evidence = project
        .modules()
        .map(|module| (module.id(), vec![Vec::new(); matchers.len()]))
        .collect::<BTreeMap<_, _>>();
    if flows.is_empty() {
        return (evidence, false, 0);
    }

    let (sources, return_budget_exhausted) = FlowSources::collect(project, &flows);
    let mut worklist = ContextWorklist::seed(project, &sources);

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
        }
        .propagate();
        if worklist.len() >= MAX_CONTEXTS {
            break;
        }
    }
    let exhausted =
        return_budget_exhausted || step_budget.exhausted() || worklist.len() >= MAX_CONTEXTS;
    if exhausted {
        for values in evidence.values_mut() {
            for rule in values {
                rule.clear();
            }
        }
    }
    (evidence, exhausted, projections)
}

impl FlowSources {
    fn collect(
        project: &ProjectSemanticModel,
        flows: &BTreeMap<FlowId, &CompiledObjectFlow>,
    ) -> (Self, bool) {
        let mut sources = Self::default();
        sources.collect_candidates(project, flows);
        let budget_exhausted = sources.refine_through_calls(project);
        (sources, budget_exhausted)
    }

    fn collect_candidates(
        &mut self,
        project: &ProjectSemanticModel,
        flows: &BTreeMap<FlowId, &CompiledObjectFlow>,
    ) {
        for module in project.modules() {
            let Some(names) = module.local().facts().names() else {
                continue;
            };
            let stream = module.local().facts().stream();
            for effect in module.local().effects().iter_effects() {
                for call in effect.calls() {
                    let cref = call.as_ref(stream);
                    for (flow_id, flow) in flows {
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

    /// Propagate source identities through returned parameter/object references
    /// and call arguments until a fixed point or the round limit is reached.
    ///
    /// Only candidates from keys that changed in the previous round are
    /// propagated; unchanged keys cannot introduce new sources at any
    /// destination.
    fn refine_through_calls(&mut self, project: &ProjectSemanticModel) -> bool {
        let mut budget = SourceBudget::new();
        let mut changed_keys: BTreeSet<SourceKey> = self.0.keys().copied().collect();

        while budget.next_round() && !changed_keys.is_empty() {
            let prev_changed = std::mem::take(&mut changed_keys);

            for module in project.modules() {
                let stream = module.local().facts().stream();
                for effect in module.local().effects().iter_effects() {
                    for call in effect.calls() {
                        let cref = call.as_ref(stream);
                        let Some((target_module, target_function)) = project
                            .qualified_function_target(
                                module.id(),
                                cref.target(),
                                cref.provenance(),
                            )
                        else {
                            continue;
                        };
                        let Some(target) = project.effect(target_module, target_function) else {
                            continue;
                        };

                        let to = SourceKey::new(module.id(), effect.id(), cref.result());

                        let mut to_changed = false;

                        for returned in target
                            .returns()
                            .iter()
                            .filter(|r| r.parameter().is_none())
                        {
                            let root = target
                                .value_root(returned.value())
                                .unwrap_or_else(|| returned.value());
                            let from = SourceKey::new(target_module, target_function, root);
                            if prev_changed.contains(&from) {
                                to_changed |= self.extend_from_key(to, &from);
                            }
                        }

                        for argument in call.arguments() {
                            if !argument.is_root()
                                || !target.returns().iter().any(|returned| {
                                    returned.parameter().is_some_and(|parameter| {
                                        parameter.index() == argument.index()
                                            && parameter.is_root()
                                    })
                                })
                            {
                                continue;
                            }
                            let root = effect
                                .value_root(argument.value())
                                .unwrap_or_else(|| argument.value());
                            let from = SourceKey::new(module.id(), effect.id(), root);
                            if prev_changed.contains(&from) {
                                to_changed |= self.extend_from_key(to, &from);
                            }
                        }

                        if to_changed {
                            changed_keys.insert(to);
                        }
                    }
                }
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
        EffectUse::CallArgument { argument, .. } => {
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
        }
    }
}

pub(super) fn chain_matches(
    chain: Option<&crate::analysis::value::NamePath>,
    member: &crate::analysis::SymbolPath,
    names: &crate::analysis::name::NameTable,
) -> bool {
    let Some(member) = crate::analysis::value::NamePath::from_symbol_path(member, names) else {
        return false;
    };
    chain.is_some_and(|chain| chain == &member || chain.last_segment() == member.last_segment())
}

pub(super) fn emit(
    project: &ProjectSemanticModel,
    evidence: &mut BTreeMap<ModuleId, Vec<Vec<ClassificationEvidence>>>,
    module: ModuleId,
    flow_id: FlowId,
    state: &CrossFlowState,
    event: FactId,
    flow: &CompiledObjectFlow,
) {
    let Some(values) = evidence.get_mut(&module) else {
        return;
    };
    let seen = values[flow_id.rule_index().get()].iter().any(|existing| {
        existing
            .occurrences
            .iter()
            .any(|occurrence| occurrence.fact == Some(event.0))
            && existing.symbol == flow.symbol
            && existing.kind == MatchKind::CallArgument
    });
    if seen {
        return;
    }
    let span = project
        .fact(module, event)
        .map_or_else(crate::ByteRange::empty, |fact| fact.span);
    values[flow_id.rule_index().get()].push(ClassificationEvidence {
        kind: MatchKind::CallArgument,
        symbol: flow.evidence_symbol(),
        count: 1,
        evidence_truncated: false,
        occurrences: vec![
            crate::api::classification::ClassificationEvidenceOccurrence {
                span,
                fact: Some(event.0),
            },
        ],
        related: related_evidence(state, module, event),
    });
    let _ = state;
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
    fn extend_from_key_copies_between_distinct_keys() {
        let mut sources = FlowSources::default();
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(from, candidate(0, 0, 10));
        sources.add(from, candidate(0, 0, 20));

        assert!(sources.extend_from_key(to, &from));

        let dest = sources.get(&to).unwrap();
        assert_eq!(dest.len(), 2);
        assert!(dest.contains(&candidate(0, 0, 10)));
        assert!(dest.contains(&candidate(0, 0, 20)));
    }

    #[test]
    fn extend_from_key_deduplicates_by_construction() {
        let mut sources = FlowSources::default();
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(from, candidate(0, 0, 10));

        assert!(sources.extend_from_key(to, &from));
        assert_eq!(sources.get(&to).unwrap().len(), 1);

        assert!(!sources.extend_from_key(to, &from));
        assert_eq!(sources.get(&to).unwrap().len(), 1);
    }

    #[test]
    fn extend_from_key_partial_novelty() {
        let mut sources = FlowSources::default();
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(from, candidate(0, 0, 10));
        sources.add(from, candidate(0, 0, 20));
        sources.add(to, candidate(0, 0, 10));

        assert!(sources.extend_from_key(to, &from));
        assert_eq!(sources.get(&to).unwrap().len(), 2);

        assert!(!sources.extend_from_key(to, &from));
    }

    #[test]
    fn extend_from_key_missing_source_returns_false() {
        let mut sources = FlowSources::default();
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        assert!(!sources.extend_from_key(to, &from));
        assert!(sources.get(&to).is_none());
    }

    #[test]
    fn extend_from_key_same_key_is_no_op() {
        let mut sources = FlowSources::default();
        let k = key(1, 1, 1);
        sources.add(k, candidate(0, 0, 10));

        assert!(!sources.extend_from_key(k, &k));
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

    #[test]
    fn extend_from_key_preserves_ordering_at_destination() {
        let mut sources = FlowSources::default();
        let from = key(1, 1, 1);
        let to = key(1, 1, 2);

        sources.add(to, candidate(0, 0, 5));
        sources.add(from, candidate(0, 1, 20));
        sources.add(from, candidate(0, 0, 10));

        sources.extend_from_key(to, &from);

        let ordered: Vec<_> = sources.get(&to).unwrap().iter().copied().collect();
        assert_eq!(ordered[0], candidate(0, 0, 5));
        assert_eq!(ordered[1], candidate(0, 0, 10));
        assert_eq!(ordered[2], candidate(0, 1, 20));
    }
}
