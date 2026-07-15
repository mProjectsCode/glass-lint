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

#[allow(clippy::too_many_lines)]
pub(in crate::analysis) fn collect(
    project: &ProjectSemanticModel,
    matchers: &CompiledMatcherCatalog<'_>,
) -> (BTreeMap<ModuleId, Vec<Vec<ApiEvidence>>>, bool) {
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
        return (evidence, false);
    }

    let mut sources = BTreeMap::<(ModuleId, FunctionId, ValueId), Vec<(FlowId, FactId)>>::new();
    for module in project.modules() {
        for effect in module.local.effects.by_id.values() {
            for call in &effect.calls {
                for (flow_id, flow) in &flows {
                    if is_source(flow, call) {
                        sources
                            .entry((module.id, effect.id, call.result))
                            .or_default()
                            .push((*flow_id, call.event));
                    }
                }
            }
        }
    }
    // Returned parameter/object identities are composed before invocation
    // contexts are seeded. The bounded monotone loop also handles a chain of
    // tiny forwarding helpers without revisiting any AST.
    let mut return_budget_exhausted = true;
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
                    for argument in &call.arguments {
                        if !argument.path.is_empty()
                            || !target.returns.iter().any(|returned| {
                                returned.index == argument.index && returned.path.is_empty()
                            })
                        {
                            continue;
                        }
                        let root = effect
                            .value_roots
                            .get(&argument.value)
                            .copied()
                            .unwrap_or(argument.value);
                        let Some(candidates) = sources.get(&(module.id, effect.id, root)).cloned()
                        else {
                            continue;
                        };
                        let entry = sources
                            .entry((module.id, effect.id, call.result))
                            .or_default();
                        let before = entry.len();
                        entry.extend(candidates);
                        entry.sort_unstable();
                        entry.dedup();
                        changed |= entry.len() != before;
                    }
                }
            }
        }
        if !changed {
            return_budget_exhausted = false;
            break;
        }
    }
    for values in sources.values_mut() {
        values.sort_unstable();
        values.dedup();
    }
    let mut queue = VecDeque::new();
    let mut seen = BTreeSet::new();
    // Seed direct effect contexts as well. A returned value gets a fresh
    // caller-side identity, so it must be projected even when no further
    // qualified call consumes it.
    for ((module, function, value), candidates) in &sources {
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
                        enqueue_parameters(
                            project,
                            target_module,
                            target_function,
                            argument.index,
                            &state,
                            target_module != module.id,
                            &mut queue,
                            &mut seen,
                        );
                    }
                }
            }
        }
    }

    let mut steps = 0usize;
    while let Some(context) = queue.pop_front() {
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
        for usage in &effect.uses {
            if !usage_matches_context(project, context.module, effect, usage, &context) {
                continue;
            }
            match usage {
                EffectUse::PropertyWrite {
                    event,
                    receiver,
                    property,
                    static_value,
                    ..
                } => {
                    let mut next = current_state.clone();
                    for (index, requirement) in flow.requirements.iter().enumerate() {
                        if let CompiledObjectRequirement::PropertyWrite {
                            property: expected,
                            value,
                        } = requirement
                            && property.as_deref() == Some(expected.as_str())
                            && value.matches_flow_value(static_value.as_deref())
                        {
                            next.requirements.insert(
                                index,
                                QualifiedEvent {
                                    module: context.module,
                                    fact: *event,
                                },
                            );
                        }
                    }
                    if flow.emit_on_requirements && ready(flow, &next) && context.crossed {
                        emit(
                            project,
                            &mut evidence,
                            context.module,
                            context.state.flow,
                            &next,
                            *event,
                            flow,
                        );
                    }
                    current_state = next;
                    let _ = receiver;
                }
                EffectUse::CallReceiver {
                    event,
                    chain,
                    receiver,
                    call_arguments,
                } => {
                    let mut next = current_state.clone();
                    for (index, requirement) in flow.requirements.iter().enumerate() {
                        if let CompiledObjectRequirement::MemberCall { member, arguments } =
                            requirement
                            && chain_matches(chain.as_deref(), member)
                            && arguments.iter().all(|matcher| {
                                call_arguments
                                    .get(matcher.index)
                                    .is_some_and(|argument| matcher.matcher.matches(argument))
                            })
                        {
                            next.requirements.insert(
                                index,
                                QualifiedEvent {
                                    module: context.module,
                                    fact: *event,
                                },
                            );
                        }
                    }
                    if flow.emit_on_requirements && ready(flow, &next) && context.crossed {
                        emit(
                            project,
                            &mut evidence,
                            context.module,
                            context.state.flow,
                            &next,
                            *event,
                            flow,
                        );
                    }
                    current_state = next;
                    let _ = receiver;
                }
                EffectUse::CallArgument {
                    event,
                    chain,
                    rooted,
                    argument,
                } => {
                    if matching_sink(flow, chain.as_deref(), *rooted, argument.index).is_some()
                        && ready(flow, &current_state)
                        && context.crossed
                    {
                        emit(
                            project,
                            &mut evidence,
                            context.module,
                            context.state.flow,
                            &current_state,
                            *event,
                            flow,
                        );
                    }
                }
            }
        }
        if seen.len() >= MAX_CONTEXTS {
            break;
        }
        for call in &effect.calls {
            let Some((target_module, target_function)) =
                project.qualified_function_target(context.module, call.target, &call.provenance)
            else {
                continue;
            };
            for argument in &call.arguments {
                if argument.parameter.as_ref().is_some_and(|parameter| {
                    context
                        .parameter
                        .is_some_and(|index| parameter.index == index)
                        && parameter.path.is_empty()
                        && argument.path.is_empty()
                }) || (context.parameter.is_none()
                    && context.source_root.is_some_and(|root| {
                        effect
                            .value_roots
                            .get(&argument.value)
                            .copied()
                            .unwrap_or(argument.value)
                            == root
                    })
                    && argument.path.is_empty())
                {
                    enqueue_parameters(
                        project,
                        target_module,
                        target_function,
                        argument.index,
                        &context.state,
                        context.crossed || target_module != context.module,
                        &mut queue,
                        &mut seen,
                    );
                }
            }
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
    (evidence, exhausted)
}

#[allow(clippy::too_many_arguments)]
fn enqueue_parameters(
    project: &ProjectSemanticModel,
    module: ModuleId,
    function: FunctionId,
    argument_index: usize,
    state: &State,
    crossed: bool,
    queue: &mut VecDeque<Context>,
    seen: &mut BTreeSet<Context>,
) {
    let Some(effect) = project
        .modules()
        .find(|candidate| candidate.id == module)
        .and_then(|candidate| candidate.local.effects.get(function))
    else {
        return;
    };
    for parameter in effect.parameters.iter().filter(|parameter| {
        parameter.parameter_index == argument_index && parameter.path.is_empty()
    }) {
        let context = Context {
            module,
            function,
            parameter: Some(parameter.parameter_index),
            source_root: None,
            state: state.clone(),
            crossed,
        };
        if seen.insert(context.clone()) {
            queue.push_back(context);
        }
    }
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
    related.sort_by_key(|item| (item.module, item.event, item.kind, item.symbol.clone()));
    related.dedup();
    related.truncate(8);
    related
}
