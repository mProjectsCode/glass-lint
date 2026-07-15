//! Project-level flow projection over qualified function effects.
//!
//! This pass is deliberately small and bounded. Local object flow remains the
//! source of truth for one module; this overlay carries a proven object state
//! through parameter-to-call relations and qualified call edges.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::effect::{EffectCall, EffectUse, FunctionEffect};
use super::index::FlowId;
use crate::analysis::ProjectSemanticModel;
use crate::analysis::facts::FactId;
use crate::analysis::value::{FunctionId, ValueId};
use crate::api::classification::{ApiEvidence, ApiMatchKind, ApiRelatedEvidence};
use crate::api::compiler::{CompiledMatcherCatalog, CompiledObjectFlow, CompiledObjectRequirement};
use crate::project::ModuleId;

const MAX_CONTEXTS: usize = 65_536;
const MAX_STEPS: usize = 262_144;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct QualifiedEvent {
    module: ModuleId,
    fact: FactId,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct State {
    flow: FlowId,
    source: QualifiedEvent,
    requirements: BTreeMap<usize, QualifiedEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Context {
    module: ModuleId,
    function: FunctionId,
    parameter: Option<usize>,
    source_root: Option<ValueId>,
    state: State,
    crossed: bool,
}

type SourceKey = (ModuleId, FunctionId, ValueId);
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
            flows.insert(
                FlowId {
                    rule_index,
                    flow_index,
                },
                flow,
            );
        }
    }
    let mut evidence = project
        .modules()
        .map(|module| (module.id, vec![Vec::new(); matchers.len()]))
        .collect::<BTreeMap<_, _>>();
    if flows.is_empty() {
        return (evidence, false, 0);
    }

    let (sources, return_budget_exhausted) = collect_sources(project, &flows);
    let (mut queue, mut seen) = seed_contexts(project, &sources);

    let mut steps = 0usize;
    let mut projections = 0usize;
    while let Some(context) = queue.pop_front() {
        projections = projections.saturating_add(1);
        steps = steps.saturating_add(1);
        if steps > MAX_STEPS {
            break;
        }
        let Some(effect) = project
            .modules()
            .find(|module| module.id == context.module)
            .and_then(|module| module.local.effects.get(context.function))
        else {
            continue;
        };
        if effect.invalid {
            continue;
        }
        let Some(flow) = flows.get(&context.state.flow).copied() else {
            continue;
        };
        let mut current_state = context.state.clone();
        let mut propagated_calls = BTreeSet::new();
        project_usages(&mut UsageProjection {
            project,
            evidence: &mut evidence,
            context: &context,
            effect,
            flow,
            state: &mut current_state,
            propagated: &mut propagated_calls,
            queue: &mut queue,
            seen: &mut seen,
        });
        propagate_calls_at(&mut Propagation {
            project,
            effect,
            module: context.module,
            context: &context,
            propagated: &mut propagated_calls,
            through: None,
            state: &current_state,
            queue: &mut queue,
            seen: &mut seen,
        });
        if seen.len() >= MAX_CONTEXTS {
            break;
        }
    }
    let exhausted = return_budget_exhausted || steps > MAX_STEPS || seen.len() >= MAX_CONTEXTS;
    if exhausted {
        for values in evidence.values_mut() {
            for rule in values {
                rule.clear();
            }
        }
    }
    (evidence, exhausted, projections)
}

struct UsageProjection<'a> {
    project: &'a ProjectSemanticModel,
    evidence: &'a mut BTreeMap<ModuleId, Vec<Vec<ApiEvidence>>>,
    context: &'a Context,
    effect: &'a FunctionEffect,
    flow: &'a CompiledObjectFlow,
    state: &'a mut State,
    propagated: &'a mut BTreeSet<FactId>,
    queue: &'a mut VecDeque<Context>,
    seen: &'a mut BTreeSet<Context>,
}

fn project_usages(input: &mut UsageProjection<'_>) {
    for usage in &input.effect.uses {
        if !usage_matches_context(
            input.project,
            input.context.module,
            input.effect,
            usage,
            input.context,
        ) {
            continue;
        }
        propagate_calls_at(&mut Propagation {
            project: input.project,
            effect: input.effect,
            module: input.context.module,
            context: input.context,
            propagated: input.propagated,
            through: Some(effect_use_event(usage)),
            state: input.state,
            queue: input.queue,
            seen: input.seen,
        });
        match usage {
            EffectUse::PropertyWrite {
                event,
                property,
                static_value,
                ..
            } => apply_property_usage(input, *event, property.as_ref(), static_value.as_ref()),
            EffectUse::CallReceiver {
                event,
                chain,
                call_arguments,
                ..
            } => apply_receiver_usage(input, *event, chain.as_ref(), call_arguments),
            EffectUse::CallArgument {
                event,
                chain,
                rooted,
                argument,
            } => apply_argument_usage(input, *event, chain.as_ref(), *rooted, argument.index),
        }
    }
}

fn apply_property_usage(
    input: &mut UsageProjection<'_>,
    event: FactId,
    property: Option<&String>,
    static_value: Option<&String>,
) {
    let mut next = input.state.clone();
    for (index, requirement) in input.flow.requirements.iter().enumerate() {
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
                    module: input.context.module,
                    fact: event,
                },
            );
        }
    }
    emit_requirements(input, &next, event);
    *input.state = next;
}

fn apply_receiver_usage(
    input: &mut UsageProjection<'_>,
    event: FactId,
    chain: Option<&String>,
    call_arguments: &[crate::analysis::facts::CallArgInfo],
) {
    let mut next = input.state.clone();
    for (index, requirement) in input.flow.requirements.iter().enumerate() {
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
                    module: input.context.module,
                    fact: event,
                },
            );
        }
    }
    emit_requirements(input, &next, event);
    *input.state = next;
}

fn apply_argument_usage(
    input: &mut UsageProjection<'_>,
    event: FactId,
    chain: Option<&String>,
    rooted: bool,
    argument_index: usize,
) {
    if matching_sink(
        input.flow,
        chain.map(String::as_str),
        rooted,
        argument_index,
    )
    .is_some()
        && ready(input.flow, input.state)
        && input.context.crossed
    {
        emit(
            input.project,
            input.evidence,
            input.context.module,
            input.context.state.flow,
            input.state,
            event,
            input.flow,
        );
    }
}

fn emit_requirements(input: &mut UsageProjection<'_>, state: &State, event: FactId) {
    if input.flow.emit_on_requirements && ready(input.flow, state) && input.context.crossed {
        emit(
            input.project,
            input.evidence,
            input.context.module,
            input.context.state.flow,
            state,
            event,
            input.flow,
        );
    }
}

fn collect_sources(
    project: &ProjectSemanticModel,
    flows: &BTreeMap<FlowId, &CompiledObjectFlow>,
) -> (FlowSources, bool) {
    let mut sources = FlowSources::default();
    for module in project.modules() {
        for effect in module.local.effects.by_id.values() {
            for call in &effect.calls {
                for (flow_id, flow) in flows {
                    if is_source(flow, call) {
                        sources.add((module.id, effect.id, call.result), (*flow_id, call.event));
                    }
                }
            }
        }
    }
    // Returned parameter/object identities are composed before invocation
    // contexts are seeded. The bounded monotone loop also handles forwarding
    // helpers without revisiting an AST.
    let mut budget_exhausted = true;
    for _ in 0..64 {
        let mut changed = false;
        for module in project.modules() {
            for effect in module.local.effects.by_id.values() {
                for call in &effect.calls {
                    let Some((target_module, target_function)) =
                        project.qualified_function_target(module.id, call.target, &call.provenance)
                    else {
                        continue;
                    };
                    let Some(target) = project
                        .modules()
                        .find(|candidate| candidate.id == target_module)
                        .and_then(|candidate| candidate.local.effects.get(target_function))
                    else {
                        continue;
                    };
                    for returned in target
                        .returns
                        .iter()
                        .filter(|returned| returned.parameter.is_none())
                    {
                        let returned_root = target
                            .value_roots
                            .get(&returned.value)
                            .copied()
                            .unwrap_or(returned.value);
                        let candidates = sources
                            .get(&(target_module, target_function, returned_root))
                            .cloned();
                        changed |= extend_sources(
                            &mut sources,
                            (module.id, effect.id, call.result),
                            candidates,
                        );
                    }
                    for argument in &call.arguments {
                        if !argument.path.is_empty()
                            || !target.returns.iter().any(|returned| {
                                returned.parameter.as_ref().is_some_and(|parameter| {
                                    parameter.index == argument.index && parameter.path.is_empty()
                                })
                            })
                        {
                            continue;
                        }
                        let root = effect
                            .value_roots
                            .get(&argument.value)
                            .copied()
                            .unwrap_or(argument.value);
                        let candidates = sources.get(&(module.id, effect.id, root)).cloned();
                        changed |= extend_sources(
                            &mut sources,
                            (module.id, effect.id, call.result),
                            candidates,
                        );
                    }
                }
            }
        }
        if !changed {
            budget_exhausted = false;
            break;
        }
    }
    sources.normalize();
    (sources, budget_exhausted)
}

fn seed_contexts(
    project: &ProjectSemanticModel,
    sources: &FlowSources,
) -> (VecDeque<Context>, BTreeSet<Context>) {
    let mut queue = VecDeque::new();
    let mut seen = BTreeSet::new();
    // A returned value gets a fresh caller-side identity and therefore needs
    // a direct context even when no qualified call consumes it.
    for ((module, function, value), candidates) in sources.iter() {
        for (flow, source_fact) in candidates {
            let context = Context {
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
                    requirements: BTreeMap::new(),
                },
                crossed: *value != source_fact_value(project, *module, *source_fact),
            };
            if seen.insert(context.clone()) {
                queue.push_back(context);
            }
        }
    }
    for module in project.modules() {
        for effect in module.local.effects.by_id.values() {
            for call in &effect.calls {
                let Some((target_module, target_function)) =
                    project.qualified_function_target(module.id, call.target, &call.provenance)
                else {
                    continue;
                };
                for argument in &call.arguments {
                    if !argument.path.is_empty() {
                        continue;
                    }
                    let root = effect
                        .value_roots
                        .get(&argument.value)
                        .copied()
                        .unwrap_or(argument.value);
                    let Some(candidates) = sources.get(&(module.id, effect.id, root)) else {
                        continue;
                    };
                    for (flow, source_fact) in candidates {
                        let state = State {
                            flow: *flow,
                            source: QualifiedEvent {
                                module: module.id,
                                fact: *source_fact,
                            },
                            requirements: BTreeMap::new(),
                        };
                        enqueue_parameters(&mut ParameterEnqueue {
                            project,
                            module: target_module,
                            function: target_function,
                            argument_index: argument.index,
                            state: &state,
                            crossed: target_module != module.id,
                            queue: &mut queue,
                            seen: &mut seen,
                        });
                    }
                }
            }
        }
    }
    (queue, seen)
}

fn extend_sources(
    sources: &mut FlowSources,
    key: SourceKey,
    candidates: Option<Vec<SourceCandidate>>,
) -> bool {
    let Some(candidates) = candidates else {
        return false;
    };
    sources.extend(key, candidates)
}

fn effect_use_event(usage: &EffectUse) -> FactId {
    match usage {
        EffectUse::PropertyWrite { event, .. }
        | EffectUse::CallArgument { event, .. }
        | EffectUse::CallReceiver { event, .. } => *event,
    }
}

fn propagate_calls_at(input: &mut Propagation<'_>) {
    for call in &input.effect.calls {
        if input.through.is_some_and(|event| call.event > event)
            || !input.propagated.insert(call.event)
        {
            continue;
        }
        let Some((target_module, target_function)) =
            input
                .project
                .qualified_function_target(input.module, call.target, &call.provenance)
        else {
            continue;
        };
        for argument in &call.arguments {
            let connected = argument.parameter.as_ref().is_some_and(|parameter| {
                input
                    .context
                    .parameter
                    .is_some_and(|index| parameter.index == index)
                    && parameter.path.is_empty()
                    && argument.path.is_empty()
            }) || (input.context.parameter.is_none()
                && input.context.source_root.is_some_and(|root| {
                    input
                        .effect
                        .value_roots
                        .get(&argument.value)
                        .copied()
                        .unwrap_or(argument.value)
                        == root
                })
                && argument.path.is_empty());
            if connected {
                enqueue_parameters(&mut ParameterEnqueue {
                    project: input.project,
                    module: target_module,
                    function: target_function,
                    argument_index: argument.index,
                    state: input.state,
                    crossed: input.context.crossed || target_module != input.module,
                    queue: input.queue,
                    seen: input.seen,
                });
            }
        }
    }
}

fn enqueue_parameters(input: &mut ParameterEnqueue<'_>) {
    let Some(effect) = input
        .project
        .modules()
        .find(|candidate| candidate.id == input.module)
        .and_then(|candidate| candidate.local.effects.get(input.function))
    else {
        return;
    };
    for parameter in effect.parameters.iter().filter(|parameter| {
        parameter.parameter_index == input.argument_index && parameter.path.is_empty()
    }) {
        let context = Context {
            module: input.module,
            function: input.function,
            parameter: Some(parameter.parameter_index),
            source_root: None,
            state: input.state.clone(),
            crossed: input.crossed,
        };
        if input.seen.insert(context.clone()) {
            input.queue.push_back(context);
        }
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
    queue: &'a mut VecDeque<Context>,
    seen: &'a mut BTreeSet<Context>,
}

struct ParameterEnqueue<'a> {
    project: &'a ProjectSemanticModel,
    module: ModuleId,
    function: FunctionId,
    argument_index: usize,
    state: &'a State,
    crossed: bool,
    queue: &'a mut VecDeque<Context>,
    seen: &'a mut BTreeSet<Context>,
}

fn is_source(flow: &CompiledObjectFlow, call: &EffectCall) -> bool {
    flow.sources.iter().any(|source| {
        call.chain.as_deref() == Some(source.member_call.as_str())
            && source.provenance.matches_rooted(call.rooted)
            && source.arguments.iter().all(|matcher| {
                call.call_arguments
                    .get(matcher.index)
                    .is_some_and(|arg| matcher.matcher.matches(arg))
            })
    })
}

fn usage_matches_context(
    _project: &ProjectSemanticModel,
    _module: ModuleId,
    effect: &FunctionEffect,
    usage: &EffectUse,
    context: &Context,
) -> bool {
    match usage {
        EffectUse::PropertyWrite {
            receiver, value, ..
        } => {
            receiver.as_ref().is_some_and(|parameter| {
                context
                    .parameter
                    .is_some_and(|index| parameter.index == index && parameter.path.is_empty())
            }) || (context.parameter.is_none()
                && context.source_root.is_some_and(|root| {
                    effect.value_roots.get(value).copied().unwrap_or(*value) == root
                }))
        }
        EffectUse::CallReceiver { receiver, .. } => context
            .parameter
            .is_some_and(|index| receiver.index == index && receiver.path.is_empty()),
        EffectUse::CallArgument { argument, .. } => {
            argument.parameter.as_ref().is_some_and(|parameter| {
                context
                    .parameter
                    .is_some_and(|index| parameter.index == index && parameter.path.is_empty())
            }) || (context.parameter.is_none()
                && context.source_root.is_some_and(|root| {
                    effect
                        .value_roots
                        .get(&argument.value)
                        .copied()
                        .unwrap_or(argument.value)
                        == root
                }))
        }
    }
}

fn source_fact_value(project: &ProjectSemanticModel, module: ModuleId, fact: FactId) -> ValueId {
    project
        .modules()
        .find(|candidate| candidate.id == module)
        .and_then(|candidate| {
            candidate
                .local
                .effects
                .by_id
                .values()
                .flat_map(|effect| effect.calls.iter())
                .find(|call| call.event == fact)
        })
        .map_or(ValueId::UNKNOWN, |call| call.result)
}

fn chain_matches(chain: Option<&str>, member: &str) -> bool {
    chain.is_some_and(|chain| chain == member || chain.rsplit('.').next() == Some(member))
}

fn matching_sink(
    flow: &CompiledObjectFlow,
    chain: Option<&str>,
    rooted: bool,
    argument: usize,
) -> Option<()> {
    flow.sinks.iter().find_map(|sink| {
        sink.member_calls
            .iter()
            .any(|member| chain == Some(member.as_str()))
            .then_some(())
            .filter(|()| sink.provenance.matches_rooted(rooted))
            .filter(|()| match &sink.args {
                crate::api::compiler::CompiledObjectSinkArgs::Any => true,
                crate::api::compiler::CompiledObjectSinkArgs::Indices(indices) => {
                    indices.contains(&argument)
                }
            })
    })
}

fn ready(flow: &CompiledObjectFlow, state: &State) -> bool {
    if flow.all_requirements_required {
        state.requirements.len() == flow.requirements.len()
    } else {
        !state.requirements.is_empty()
    }
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
    let seen = values[flow_id.rule_index].iter().any(|existing| {
        existing.event_ids == vec![event.0]
            && existing.symbol == flow.symbol
            && existing.kind == ApiMatchKind::CallArgument
    });
    if seen {
        return;
    }
    let span = project
        .modules()
        .find(|candidate| candidate.id == module)
        .and_then(|candidate| candidate.local.facts.stream.fact(event))
        .map_or_else(
            || swc_common::Span::new(swc_common::BytePos(0), swc_common::BytePos(0)),
            |fact| fact.span,
        );
    values[flow_id.rule_index].push(ApiEvidence {
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
