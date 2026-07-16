//! Project-level flow projection over qualified function effects.
//!
//! This pass is deliberately small and bounded. Local object flow remains the
//! source of truth for one module; this overlay carries a proven object state
//! through parameter-to-call relations and qualified call edges.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::{
    effect::{EffectUse, FunctionEffect},
    index::FlowId,
    requirements::RequirementSet,
};
use crate::{
    analysis::{
        ProjectSemanticModel,
        facts::FactId,
        value::{FunctionId, ValueId},
    },
    api::{
        classification::{ApiEvidence, ApiMatchKind, ApiRelatedEvidence},
        compiler::{CompiledMatcherCatalog, CompiledObjectFlow, CompiledObjectRequirement},
    },
    project::ModuleId,
};

const MAX_CONTEXTS: usize = 65_536;
const MAX_STEPS: usize = 262_144;

#[derive(Debug, Default)]
/// Global work budget for qualified flow propagation.
///
/// Exhaustion invalidates the collected evidence because a partial cross-file
/// result cannot distinguish “not reached” from “not analyzed.”
struct CrossBudget {
    /// Number of propagation steps consumed.
    steps: usize,
    /// Number of contexts projected into evidence.
    projections: usize,
    /// Whether the hard step limit was reached.
    exhausted: bool,
}

impl CrossBudget {
    fn step(&mut self) -> bool {
        // Cross-module propagation is monotone but can fan out through helper
        // chains; stop before that fan-out can make analysis unbounded.
        self.steps = match self.steps.checked_add(1) {
            Some(value) if value <= MAX_STEPS => value,
            _ => {
                self.exhausted = true;
                return false;
            }
        };
        true
    }

    fn projection(&mut self) {
        self.projections = self.projections.saturating_add(1);
    }
}

#[derive(Debug)]
/// Fixed-point budget for propagating source identities through helper calls.
struct SourceBudget {
    /// Number of refinement rounds performed.
    rounds: usize,
    /// Whether stabilization was not reached before the limit.
    exhausted: bool,
}

impl SourceBudget {
    fn new() -> Self {
        Self {
            rounds: 0,
            exhausted: true,
        }
    }

    fn next_round(&mut self) -> bool {
        // Source identities are refined to a fixed point, with a hard round
        // limit that reports exhaustion rather than guessing a partial state.
        self.rounds = self.rounds.saturating_add(1);
        self.rounds <= 64
    }

    fn stabilized(&mut self) {
        self.exhausted = false;
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
/// A fact location qualified by its owning project module.
struct QualifiedEvent {
    module: ModuleId,
    fact: FactId,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
/// Monotone flow state carried through one qualified call context.
struct State {
    flow: FlowId,
    source: QualifiedEvent,
    requirements: RequirementSet<QualifiedEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
/// Worklist context identifying the function/value path currently projected.
struct Context {
    module: ModuleId,
    function: FunctionId,
    parameter: Option<usize>,
    source_root: Option<ValueId>,
    state: State,
    crossed: bool,
}

#[derive(Default)]
/// Deduplicating FIFO worklist for bounded interprocedural contexts.
struct ContextWorklist {
    pending: VecDeque<Context>,
    seen: BTreeSet<Context>,
}

impl ContextWorklist {
    fn push(&mut self, context: Context) {
        if self.seen.insert(context.clone()) {
            self.pending.push_back(context);
        }
    }

    fn pop_front(&mut self) -> Option<Context> {
        self.pending.pop_front()
    }

    fn len(&self) -> usize {
        self.seen.len()
    }

    fn enqueue_parameters(
        &mut self,
        project: &ProjectSemanticModel,
        module: ModuleId,
        function: FunctionId,
        argument_index: usize,
        state: &State,
        crossed: bool,
    ) {
        let Some(effect) = project
            .modules()
            .find(|candidate| candidate.id() == module)
            .and_then(|candidate| candidate.local().effects().get(function))
        else {
            return;
        };
        for parameter in effect.parameters().iter().filter(|parameter| {
            parameter.parameter_index == argument_index && parameter.path.is_empty()
        }) {
            self.push(Context {
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
        // A returned value gets a fresh caller-side identity and therefore
        // needs a direct context even when no qualified call consumes it.
        for ((module, function, value), candidates) in sources.iter() {
            for (flow, source_fact) in candidates {
                worklist.push(Context {
                    module: *module,
                    function: *function,
                    parameter: None,
                    source_root: Some(*value),
                    state: State {
                        flow: *flow,
                        source: QualifiedEvent {
                            module: *module,
                            fact: *source_fact,
                        },
                        requirements: RequirementSet::default(),
                    },
                    crossed: *value != project.source_call_result(*module, *source_fact),
                });
            }
        }
        for module in project.modules() {
            for effect in module.local().effects().iter_effects() {
                for call in effect.calls() {
                    let Some((target_module, target_function)) = project.qualified_function_target(
                        module.id(),
                        call.target(),
                        call.provenance(),
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
                        let Some(candidates) = sources.get(&(module.id(), effect.id(), root))
                        else {
                            continue;
                        };
                        for (flow, source_fact) in candidates {
                            let state = State {
                                flow: *flow,
                                source: QualifiedEvent {
                                    module: module.id(),
                                    fact: *source_fact,
                                },
                                requirements: RequirementSet::default(),
                            };
                            worklist.enqueue_parameters(
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
        worklist
    }
}

/// Local effect/value key used while composing source identities.
type SourceKey = (ModuleId, FunctionId, ValueId);
/// Flow matcher and source-event pair associated with a source identity.
type SourceCandidate = (FlowId, FactId);

/// Proven source identities indexed by the local effect that produced them.
/// Keeping insertion, transitive extension, and normalization here prevents
/// callers from having to repeat the map invariants.
#[derive(Default)]
struct FlowSources(BTreeMap<SourceKey, Vec<SourceCandidate>>);

impl FlowSources {
    fn add(&mut self, key: SourceKey, candidate: SourceCandidate) {
        self.0.entry(key).or_default().push(candidate);
    }

    fn get(&self, key: &SourceKey) -> Option<&Vec<SourceCandidate>> {
        self.0.get(key)
    }

    fn iter(&self) -> impl Iterator<Item = (&SourceKey, &Vec<SourceCandidate>)> {
        self.0.iter()
    }

    fn extend(&mut self, key: SourceKey, candidates: Vec<SourceCandidate>) -> bool {
        let entry = self.0.entry(key).or_default();
        let before = entry.len();
        entry.extend(candidates);
        entry.sort_unstable();
        entry.dedup();
        entry.len() != before
    }

    fn extend_optional(
        &mut self,
        key: SourceKey,
        candidates: Option<Vec<SourceCandidate>>,
    ) -> bool {
        candidates.is_some_and(|candidates| self.extend(key, candidates))
    }

    fn normalize(&mut self) {
        for values in self.0.values_mut() {
            values.sort_unstable();
            values.dedup();
        }
    }
}

pub(in crate::analysis) fn collect(
    project: &ProjectSemanticModel,
    matchers: &CompiledMatcherCatalog<'_>,
) -> (BTreeMap<ModuleId, Vec<Vec<ApiEvidence>>>, bool, usize) {
    let mut flows = BTreeMap::<FlowId, &CompiledObjectFlow>::new();
    for (rule_index, matcher) in matchers.selected_matchers() {
        for (flow_index, flow) in matcher.flows.iter().enumerate() {
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

    let mut budget = CrossBudget::default();
    while let Some(context) = worklist.pop_front() {
        budget.projection();
        if !budget.step() {
            break;
        }
        let Some(effect) = project
            .modules()
            .find(|module| module.id() == context.module)
            .and_then(|module| module.local().effects().get(context.function))
        else {
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
        UsageProjection {
            project,
            evidence: &mut evidence,
            context: &context,
            effect,
            flow,
            state: &mut current_state,
            propagated: &mut propagated_calls,
            worklist: &mut worklist,
        }
        .project();
        Propagation {
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
    let exhausted = return_budget_exhausted || budget.exhausted || worklist.len() >= MAX_CONTEXTS;
    if exhausted {
        for values in evidence.values_mut() {
            for rule in values {
                rule.clear();
            }
        }
    }
    (evidence, exhausted, budget.projections)
}

struct UsageProjection<'a> {
    project: &'a ProjectSemanticModel,
    evidence: &'a mut BTreeMap<ModuleId, Vec<Vec<ApiEvidence>>>,
    context: &'a Context,
    effect: &'a FunctionEffect,
    flow: &'a CompiledObjectFlow,
    state: &'a mut State,
    propagated: &'a mut BTreeSet<FactId>,
    worklist: &'a mut ContextWorklist,
}

impl UsageProjection<'_> {
    fn project(&mut self) {
        for usage in self.effect.uses() {
            if !usage_matches_context(self.effect, usage, self.context) {
                continue;
            }
            Propagation {
                project: self.project,
                effect: self.effect,
                module: self.context.module,
                context: self.context,
                propagated: self.propagated,
                through: Some(effect_use_event(usage)),
                state: self.state,
                worklist: self.worklist,
            }
            .propagate();
            match usage {
                EffectUse::PropertyWrite {
                    event,
                    property,
                    static_value,
                    ..
                } => self.apply_property(*event, property.as_ref(), static_value.as_ref()),
                EffectUse::CallReceiver {
                    event,
                    chain,
                    call_arguments,
                    ..
                } => self.apply_receiver(*event, chain.as_ref(), call_arguments),
                EffectUse::CallArgument {
                    event,
                    chain,
                    rooted,
                    argument,
                } => self.apply_argument(*event, chain.as_ref(), *rooted, argument.index()),
            }
        }
    }

    fn apply_property(
        &mut self,
        event: FactId,
        property: Option<&String>,
        static_value: Option<&String>,
    ) {
        let mut next = self.state.clone();
        for (index, requirement) in self.flow.requirements.iter().enumerate() {
            if let CompiledObjectRequirement::PropertyWrite {
                property: expected,
                value,
            } = requirement
                && property == Some(expected)
                && value.matches_flow_value(static_value.map(String::as_str))
            {
                next.requirements.insert(
                    index,
                    QualifiedEvent {
                        module: self.context.module,
                        fact: event,
                    },
                );
            }
        }
        self.emit_requirements(&next, event);
        *self.state = next;
    }

    fn apply_receiver(
        &mut self,
        event: FactId,
        chain: Option<&String>,
        call_arguments: &[crate::analysis::facts::CallArgInfo],
    ) {
        let mut next = self.state.clone();
        for (index, requirement) in self.flow.requirements.iter().enumerate() {
            if let CompiledObjectRequirement::MemberCall { member, arguments } = requirement
                && chain_matches(chain.map(String::as_str), member)
                && arguments.iter().all(|matcher| {
                    call_arguments
                        .get(matcher.index)
                        .is_some_and(|argument| matcher.matcher.matches(argument))
                })
            {
                next.requirements.insert(
                    index,
                    QualifiedEvent {
                        module: self.context.module,
                        fact: event,
                    },
                );
            }
        }
        self.emit_requirements(&next, event);
        *self.state = next;
    }

    fn apply_argument(
        &mut self,
        event: FactId,
        chain: Option<&String>,
        rooted: bool,
        argument: usize,
    ) {
        if self
            .flow
            .sink_matches(chain.map(String::as_str), rooted, argument)
            && self.flow.requirements_ready(self.state.requirements.len())
            && self.context.crossed
        {
            emit(
                self.project,
                self.evidence,
                self.context.module,
                self.context.state.flow,
                self.state,
                event,
                self.flow,
            );
        }
    }

    fn emit_requirements(&mut self, state: &State, event: FactId) {
        if self.flow.emit_on_requirements
            && self.flow.requirements_ready(state.requirements.len())
            && self.context.crossed
        {
            emit(
                self.project,
                self.evidence,
                self.context.module,
                self.context.state.flow,
                state,
                event,
                self.flow,
            );
        }
    }
}

impl FlowSources {
    fn collect(
        project: &ProjectSemanticModel,
        flows: &BTreeMap<FlowId, &CompiledObjectFlow>,
    ) -> (Self, bool) {
        let mut sources = Self::default();
        for module in project.modules() {
            for effect in module.local().effects().iter_effects() {
                for call in effect.calls() {
                    for (flow_id, flow) in flows {
                        if call.matches_source(flow) {
                            sources.add(
                                (module.id(), effect.id(), call.result()),
                                (*flow_id, call.event()),
                            );
                        }
                    }
                }
            }
        }
        // Returned parameter/object identities are composed before invocation
        // contexts are seeded. The bounded monotone loop also handles forwarding
        // helpers without revisiting an AST.
        let mut budget = SourceBudget::new();
        while budget.next_round() {
            let mut changed = false;
            for module in project.modules() {
                for effect in module.local().effects().iter_effects() {
                    for call in effect.calls() {
                        let Some((target_module, target_function)) = project
                            .qualified_function_target(
                                module.id(),
                                call.target(),
                                call.provenance(),
                            )
                        else {
                            continue;
                        };
                        let Some(target) = project
                            .modules()
                            .find(|candidate| candidate.id() == target_module)
                            .and_then(|candidate| candidate.local().effects().get(target_function))
                        else {
                            continue;
                        };
                        for returned in target
                            .returns()
                            .iter()
                            .filter(|returned| returned.parameter().is_none())
                        {
                            let returned_root = target
                                .value_root(returned.value())
                                .unwrap_or_else(|| returned.value());
                            let candidates = sources
                                .get(&(target_module, target_function, returned_root))
                                .cloned();
                            changed |= sources.extend_optional(
                                (module.id(), effect.id(), call.result()),
                                candidates,
                            );
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
                            let candidates =
                                sources.get(&(module.id(), effect.id(), root)).cloned();
                            changed |= sources.extend_optional(
                                (module.id(), effect.id(), call.result()),
                                candidates,
                            );
                        }
                    }
                }
            }
            if !changed {
                budget.stabilized();
                break;
            }
        }
        sources.normalize();
        (sources, budget.exhausted)
    }
}

fn effect_use_event(usage: &EffectUse) -> FactId {
    match usage {
        EffectUse::PropertyWrite { event, .. }
        | EffectUse::CallArgument { event, .. }
        | EffectUse::CallReceiver { event, .. } => *event,
    }
}

struct Propagation<'a> {
    project: &'a ProjectSemanticModel,
    effect: &'a FunctionEffect,
    module: ModuleId,
    context: &'a Context,
    propagated: &'a mut BTreeSet<FactId>,
    through: Option<FactId>,
    state: &'a State,
    worklist: &'a mut ContextWorklist,
}

impl Propagation<'_> {
    fn propagate(&mut self) {
        for call in self.effect.calls() {
            if self.through.is_some_and(|event| call.event() > event)
                || !self.propagated.insert(call.event())
            {
                continue;
            }
            let Some((target_module, target_function)) = self.project.qualified_function_target(
                self.module,
                call.target(),
                call.provenance(),
            ) else {
                continue;
            };
            for argument in call.arguments() {
                let connected = argument.parameter().is_some_and(|parameter| {
                    self.context
                        .parameter
                        .is_some_and(|index| parameter.index() == index)
                        && parameter.is_root()
                        && argument.is_root()
                }) || (self.context.parameter.is_none()
                    && self.context.source_root.is_some_and(|root| {
                        self.effect
                            .value_root(argument.value())
                            .unwrap_or_else(|| argument.value())
                            == root
                    })
                    && argument.is_root());
                if connected {
                    self.worklist.enqueue_parameters(
                        self.project,
                        target_module,
                        target_function,
                        argument.index(),
                        self.state,
                        self.context.crossed || target_module != self.module,
                    );
                }
            }
        }
    }
}

fn usage_matches_context(effect: &FunctionEffect, usage: &EffectUse, context: &Context) -> bool {
    match usage {
        EffectUse::PropertyWrite {
            receiver, value, ..
        } => {
            receiver.as_ref().is_some_and(|parameter| {
                context
                    .parameter
                    .is_some_and(|index| parameter.index() == index && parameter.is_root())
            }) || (context.parameter.is_none()
                && context
                    .source_root
                    .is_some_and(|root| effect.value_root(*value).unwrap_or(*value) == root))
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

fn chain_matches(chain: Option<&str>, member: &str) -> bool {
    chain.is_some_and(|chain| chain == member || chain.rsplit('.').next() == Some(member))
}

fn emit(
    project: &ProjectSemanticModel,
    evidence: &mut BTreeMap<ModuleId, Vec<Vec<ApiEvidence>>>,
    module: ModuleId,
    flow_id: FlowId,
    state: &State,
    event: FactId,
    flow: &CompiledObjectFlow,
) {
    let Some(values) = evidence.get_mut(&module) else {
        return;
    };
    let seen = values[flow_id.rule_index()].iter().any(|existing| {
        existing.event_ids == vec![event.0]
            && existing.symbol == flow.symbol
            && existing.kind == ApiMatchKind::CallArgument
    });
    if seen {
        return;
    }
    let span = project
        .modules()
        .find(|candidate| candidate.id() == module)
        .and_then(|candidate| candidate.local().facts().stream().fact(event))
        .map_or_else(
            || swc_common::Span::new(swc_common::BytePos(0), swc_common::BytePos(0)),
            |fact| fact.span,
        );
    values[flow_id.rule_index()].push(ApiEvidence {
        kind: ApiMatchKind::CallArgument,
        symbol: flow.evidence_symbol(),
        count: 1,
        spans: vec![span],
        event_ids: vec![event.0],
        related: related_evidence(state, module, event),
    });
    let _ = state;
}

fn related_evidence(
    state: &State,
    sink_module: ModuleId,
    sink_event: FactId,
) -> Vec<ApiRelatedEvidence> {
    let mut related = vec![ApiRelatedEvidence {
        module: state.source.module.0,
        event: state.source.fact.0,
        kind: ApiMatchKind::CallArgument,
        symbol: "flow source".into(),
    }];
    related.extend(state.requirements.values().map(|event| ApiRelatedEvidence {
        module: event.module.0,
        event: event.fact.0,
        kind: ApiMatchKind::CallArgument,
        symbol: "flow requirement".into(),
    }));
    related.push(ApiRelatedEvidence {
        module: sink_module.0,
        event: sink_event.0,
        kind: ApiMatchKind::CallArgument,
        symbol: "flow sink".into(),
    });
    let mut seen = BTreeSet::new();
    related.retain(|item| seen.insert((item.module, item.event, item.kind, item.symbol.clone())));
    related.truncate(8);
    related
}
