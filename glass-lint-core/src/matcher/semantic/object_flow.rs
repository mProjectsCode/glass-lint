//! Bounded object flow over the canonical semantic fact stream.
//!
//! This projector owns no AST and performs no resolution.  `FactBuilder` has
//! already assigned value identities, effective call arguments, member chains,
//! and function targets.  The transfer state below only follows those typed
//! identities and emits evidence at canonical call sites.

use std::collections::{BTreeMap, BTreeSet};

use super::super::result::{ApiEvidence, ApiMatchKind};
use super::super::rule::{FlowMatcher, FlowRequirement, FlowSinkArgs};
use super::facts::{CallArgInfo, ControlKind, FactId, FactPayload, FactStream, FunctionBoundary};
use super::flow_index::{FlowId, FlowIndex, FlowLimits};
use super::flow_state::{FlowState, state_is_ready};
use super::summary::{FunctionSummaries, invocation_is_compatible, project_parameter_argument};
use super::value::{ObjectId, ValueId};

pub(super) fn collect(
    stream: &FactStream,
    rules: &[(usize, usize, &FlowMatcher)],
    rule_count: usize,
) -> Vec<Vec<ApiEvidence>> {
    collect_with_limits(stream, rules, rule_count, FlowLimits::default())
}

pub(super) fn collect_with_limits(
    stream: &FactStream,
    rules: &[(usize, usize, &FlowMatcher)],
    rule_count: usize,
    limits: FlowLimits,
) -> Vec<Vec<ApiEvidence>> {
    let flow_index = FlowIndex::new(rules);
    let helpers = super::summary::collect(stream, &flow_index);
    let calls_by_result = stream
        .facts()
        .iter()
        .filter_map(|fact| match &fact.payload {
            FactPayload::Call {
                result,
                rooted_chain,
                syntactic_chain,
                callee_name,
                args,
                unwrap,
                ..
            } => {
                let (chain, effective_args) = unwrap.as_deref().map_or(
                    (
                        rooted_chain
                            .clone()
                            .or_else(|| syntactic_chain.clone())
                            .or_else(|| callee_name.clone()),
                        args.clone(),
                    ),
                    |unwrap| (Some(unwrap.chain.clone()), unwrap.effective_args.clone()),
                );
                Some((
                    *result,
                    SourceCall {
                        chain,
                        args: effective_args,
                        fact_id: fact.id,
                    },
                ))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();

    let mut projector = ObjectFlowProjector {
        flow_index,
        helpers,
        calls_by_result,
        fact_spans: stream
            .facts()
            .iter()
            .map(|fact| (fact.id, fact.span))
            .collect(),
        evidence: vec![Vec::new(); rule_count],
        aliases: BTreeMap::new(),
        states: BTreeMap::new(),
        emitted: BTreeSet::new(),
        next_object_id: 0,
        limits,
        control: Vec::new(),
        reachable: true,
    };
    for fact in stream.facts() {
        projector.transfer(fact);
    }
    projector.evidence
}

#[derive(Debug)]
struct ObjectFlowProjector<'rules> {
    flow_index: FlowIndex<'rules>,
    helpers: FunctionSummaries,
    calls_by_result: BTreeMap<ValueId, SourceCall>,
    fact_spans: BTreeMap<FactId, swc_common::Span>,
    evidence: Vec<Vec<ApiEvidence>>,
    aliases: BTreeMap<ValueId, ObjectId>,
    states: BTreeMap<(ObjectId, FlowId), FlowState>,
    emitted: BTreeSet<(usize, usize, ObjectId, FactId)>,
    next_object_id: u32,
    limits: FlowLimits,
    control: Vec<ControlFrame>,
    reachable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlowEnvironment {
    aliases: BTreeMap<ValueId, ObjectId>,
    states: BTreeMap<(ObjectId, FlowId), FlowState>,
    reachable: bool,
}

#[derive(Debug, Clone)]
enum ControlFrame {
    Branch {
        region: u32,
        base: FlowEnvironment,
        then_exit: Option<FlowEnvironment>,
    },
    Loop {
        region: u32,
        baseline: FlowEnvironment,
        guaranteed: bool,
        breaks: Vec<FlowEnvironment>,
        continues: Vec<FlowEnvironment>,
    },
    Switch {
        region: u32,
        baseline: FlowEnvironment,
        breaks: Vec<FlowEnvironment>,
        has_default: bool,
    },
    Try {
        region: u32,
        baseline: FlowEnvironment,
        try_exit: Option<FlowEnvironment>,
        catch_exit: Option<FlowEnvironment>,
        normal_exit: Option<FlowEnvironment>,
        abrupt_exits: Vec<(AbruptExit, FlowEnvironment)>,
        has_finally: bool,
    },
    Function {
        caller: FlowEnvironment,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AbruptExit {
    Break,
    Continue,
    Return,
}

#[derive(Debug, Clone)]
struct SourceCall {
    chain: Option<String>,
    args: Vec<CallArgInfo>,
    fact_id: FactId,
}

impl<'rules> ObjectFlowProjector<'rules> {
    fn transfer(&mut self, fact: &super::facts::SemanticFact) {
        match &fact.payload {
            FactPayload::Function { boundary, .. } => match boundary {
                FunctionBoundary::Enter => {
                    let caller = self.environment();
                    self.control.push(ControlFrame::Function { caller });
                    self.aliases.clear();
                    self.states.clear();
                    self.reachable = true;
                }
                FunctionBoundary::Exit => {
                    if let Some(ControlFrame::Function { caller }) = self.control.pop() {
                        self.restore(caller);
                    }
                }
            },
            FactPayload::Control { kind, region } => {
                self.transfer_control(*kind, *region, fact.span);
            }
            FactPayload::Declaration { target, source } => {
                if !self.reachable {
                    return;
                }
                self.assign(*target, *source);
            }
            FactPayload::Assignment {
                target,
                source,
                receiver,
            } => {
                if !self.reachable {
                    return;
                }
                if let Some(receiver) = receiver {
                    self.invalidate_object(*receiver);
                } else {
                    self.assign(*target, *source);
                }
            }
            FactPayload::PropertyWrite {
                receiver,
                property,
                static_value,
                ..
            } => {
                if !self.reachable {
                    return;
                }
                self.record_property_write(
                    *receiver,
                    property.as_deref(),
                    static_value.as_deref(),
                    fact.id,
                )
            }
            FactPayload::Call {
                syntactic_chain,
                rooted_chain,
                callee_name,
                receiver,
                args,
                unwrap,
                target_function,
                ..
            } => {
                if !self.reachable {
                    return;
                }
                let (chain, effective_args) = unwrap.as_deref().map_or(
                    (
                        rooted_chain
                            .as_deref()
                            .or(syntactic_chain.as_deref())
                            .or(callee_name.as_deref()),
                        args.as_slice(),
                    ),
                    |unwrap| {
                        (
                            Some(unwrap.chain.as_str()),
                            unwrap.effective_args.as_slice(),
                        )
                    },
                );
                if let Some(chain) = chain {
                    self.record_configuration(*receiver, chain, effective_args, fact.id);
                    self.record_sinks(chain, effective_args, fact.id);
                }
                if let Some(function) = target_function {
                    self.record_helper_sink(*function, args, fact.id);
                }
            }
            _ => {}
        }
    }

    fn environment(&self) -> FlowEnvironment {
        FlowEnvironment {
            aliases: self.aliases.clone(),
            states: self.states.clone(),
            reachable: self.reachable,
        }
    }

    fn restore(&mut self, environment: FlowEnvironment) {
        self.aliases = environment.aliases;
        self.states = environment.states;
        self.reachable = environment.reachable;
    }

    fn join(left: &FlowEnvironment, right: &FlowEnvironment) -> FlowEnvironment {
        if !left.reachable {
            return right.clone();
        }
        if !right.reachable {
            return left.clone();
        }
        let aliases = left
            .aliases
            .iter()
            .filter_map(|(binding, object)| {
                (right.aliases.get(binding) == Some(object)).then_some((*binding, *object))
            })
            .collect();
        let states = left
            .states
            .iter()
            .filter_map(|(key, left_state)| {
                let right_state = right.states.get(key)?;
                let mut state = left_state.clone();
                state
                    .requirements
                    .retain(|requirement, _| right_state.requirements.contains_key(requirement));
                Some((*key, state))
            })
            .collect();
        FlowEnvironment {
            aliases,
            states,
            reachable: true,
        }
    }

    fn join_many(environments: &[FlowEnvironment]) -> FlowEnvironment {
        let mut joined = environments
            .iter()
            .find(|environment| environment.reachable)
            .cloned()
            .unwrap_or(FlowEnvironment {
                aliases: BTreeMap::new(),
                states: BTreeMap::new(),
                reachable: false,
            });
        for environment in environments {
            if std::ptr::eq(&joined, environment) {
                continue;
            }
            joined = Self::join(&joined, environment);
        }
        joined
    }

    fn transfer_control(&mut self, kind: ControlKind, region: u32, _span: swc_common::Span) {
        match kind {
            ControlKind::BranchStart => self.control.push(ControlFrame::Branch {
                region,
                base: self.environment(),
                then_exit: None,
            }),
            ControlKind::BranchThen => {
                let current = self.environment();
                if let Some(ControlFrame::Branch {
                    region: expected,
                    base,
                    ..
                }) = self.control.last_mut()
                    && *expected == region
                {
                    *base = current;
                }
            }
            ControlKind::BranchElse => {
                let current = self.environment();
                let mut restore = None;
                if let Some(ControlFrame::Branch {
                    region: expected,
                    base,
                    then_exit,
                }) = self.control.last_mut()
                    && *expected == region
                {
                    *then_exit = Some(current);
                    restore = Some(base.clone());
                }
                if let Some(environment) = restore {
                    self.restore(environment);
                }
            }
            ControlKind::BranchEnd => {
                let Some(ControlFrame::Branch {
                    region: expected,
                    base,
                    then_exit,
                }) = self.control.pop()
                else {
                    return;
                };
                if expected != region {
                    return;
                }
                let current = self.environment();
                let joined = then_exit.as_ref().map_or_else(
                    || Self::join(&base, &current),
                    |then_exit| Self::join(then_exit, &current),
                );
                self.restore(joined);
            }
            ControlKind::LoopStart { guaranteed } => self.control.push(ControlFrame::Loop {
                region,
                baseline: self.environment(),
                guaranteed,
                breaks: Vec::new(),
                continues: Vec::new(),
            }),
            ControlKind::LoopUpdate => {
                let current = self.environment();
                if let Some(ControlFrame::Loop { continues, .. }) = self.control.last()
                    && !continues.is_empty()
                {
                    let mut paths = vec![current];
                    paths.extend(continues.iter().cloned());
                    self.restore(Self::join_many(&paths));
                }
            }
            ControlKind::LoopEnd => {
                let Some(ControlFrame::Loop {
                    region: expected,
                    baseline,
                    guaranteed,
                    breaks,
                    continues,
                }) = self.control.pop()
                else {
                    return;
                };
                if expected != region {
                    return;
                }
                let mut paths = Vec::new();
                if !guaranteed {
                    paths.push(baseline);
                }
                paths.extend(breaks);
                // A continue in a do/while still reaches its test and can
                // therefore reach the loop exit.
                paths.extend(continues);
                paths.push(self.environment());
                self.restore(Self::join_many(&paths));
            }
            ControlKind::SwitchStart => self.control.push(ControlFrame::Switch {
                region,
                baseline: self.environment(),
                breaks: Vec::new(),
                has_default: false,
            }),
            ControlKind::SwitchCase { is_default } => {
                let current = self.environment();
                let mut restore = None;
                if let Some(ControlFrame::Switch {
                    region: expected,
                    baseline,
                    has_default: default,
                    ..
                }) = self.control.last_mut()
                    && *expected == region
                {
                    // The current environment is the fall-through input
                    // from the preceding case. Joining it with baseline
                    // also admits direct entry at this case.
                    restore = Some(Self::join(&current, baseline));
                    *default |= is_default;
                }
                if let Some(environment) = restore {
                    self.restore(environment);
                }
            }
            ControlKind::SwitchEnd => {
                let Some(ControlFrame::Switch {
                    region: expected,
                    baseline,
                    breaks,
                    has_default,
                    ..
                }) = self.control.pop()
                else {
                    return;
                };
                if expected != region {
                    return;
                }
                let mut exits = vec![self.environment()];
                exits.extend(breaks);
                if !has_default {
                    exits.push(baseline);
                }
                self.restore(Self::join_many(&exits));
            }
            ControlKind::TryStart => self.control.push(ControlFrame::Try {
                region,
                baseline: self.environment(),
                try_exit: None,
                catch_exit: None,
                normal_exit: None,
                abrupt_exits: Vec::new(),
                has_finally: false,
            }),
            ControlKind::CatchStart => {
                let current = self.environment();
                let mut restore = None;
                if let Some(ControlFrame::Try {
                    region: expected,
                    baseline,
                    try_exit,
                    ..
                }) = self.control.last_mut()
                    && *expected == region
                {
                    *try_exit = current.reachable.then_some(current);
                    restore = Some(baseline.clone());
                }
                if let Some(environment) = restore {
                    self.restore(environment);
                }
            }
            ControlKind::FinallyStart => {
                let current = self.environment();
                let mut restore = None;
                if let Some(ControlFrame::Try {
                    region: expected,
                    try_exit,
                    catch_exit,
                    normal_exit,
                    abrupt_exits,
                    has_finally,
                    ..
                }) = self.control.last_mut()
                    && *expected == region
                {
                    *catch_exit = Some(current.clone());
                    *has_finally = true;
                    let mut normal = try_exit.clone();
                    if current.reachable {
                        normal = Some(
                            normal.map_or(current.clone(), |normal| Self::join(&normal, &current)),
                        );
                    }
                    *normal_exit = normal.clone();
                    let mut incoming = Vec::new();
                    if let Some(normal) = normal {
                        incoming.push(normal);
                    }
                    incoming.extend(
                        abrupt_exits
                            .iter()
                            .map(|(_, environment)| environment.clone()),
                    );
                    restore = Some(Self::join_many(&incoming));
                }
                if let Some(environment) = restore {
                    self.restore(environment);
                }
            }
            ControlKind::TryEnd => {
                let Some(ControlFrame::Try {
                    region: expected,
                    try_exit,
                    catch_exit,
                    normal_exit,
                    abrupt_exits,
                    has_finally,
                    ..
                }) = self.control.pop()
                else {
                    return;
                };
                if expected != region {
                    return;
                }
                if has_finally {
                    let after_finally = self.environment();
                    for (kind, before) in abrupt_exits {
                        self.apply_finally_to_abrupt_exit(kind, &before, &after_finally);
                    }
                    if let Some(normal) = normal_exit {
                        if normal.reachable {
                            self.restore(after_finally);
                        } else {
                            self.restore(FlowEnvironment {
                                aliases: BTreeMap::new(),
                                states: BTreeMap::new(),
                                reachable: false,
                            });
                        }
                    } else {
                        self.restore(FlowEnvironment {
                            aliases: BTreeMap::new(),
                            states: BTreeMap::new(),
                            reachable: false,
                        });
                    }
                    return;
                }
                if let Some(try_exit) = try_exit {
                    let catch_exit = catch_exit.unwrap_or_else(|| self.environment());
                    self.restore(Self::join(&try_exit, &catch_exit));
                }
            }
            ControlKind::Break => {
                let current = self.environment();
                self.record_abrupt_exit(AbruptExit::Break, &current);
                if let Some(frame) = self.control.iter_mut().rev().find(|frame| {
                    matches!(
                        frame,
                        ControlFrame::Loop { .. } | ControlFrame::Switch { .. }
                    )
                }) {
                    match frame {
                        ControlFrame::Loop { breaks, .. } | ControlFrame::Switch { breaks, .. } => {
                            breaks.push(current)
                        }
                        _ => unreachable!(),
                    }
                    self.reachable = false;
                }
            }
            ControlKind::Continue => {
                let current = self.environment();
                self.record_abrupt_exit(AbruptExit::Continue, &current);
                if let Some(ControlFrame::Loop { continues, .. }) = self
                    .control
                    .iter_mut()
                    .rev()
                    .find(|frame| matches!(frame, ControlFrame::Loop { .. }))
                {
                    continues.push(current);
                    self.reachable = false;
                }
            }
            ControlKind::Return => {
                let current = self.environment();
                self.record_abrupt_exit(AbruptExit::Return, &current);
                self.reachable = false;
            }
        }
    }

    fn record_abrupt_exit(&mut self, kind: AbruptExit, environment: &FlowEnvironment) {
        for frame in self.control.iter_mut().rev() {
            if let ControlFrame::Try { abrupt_exits, .. } = frame {
                abrupt_exits.push((kind, environment.clone()));
            }
        }
    }

    fn apply_finally_to_abrupt_exit(
        &mut self,
        kind: AbruptExit,
        before: &FlowEnvironment,
        after: &FlowEnvironment,
    ) {
        let frames = self.control.iter_mut().rev();
        for frame in frames {
            let targets = match (kind, frame) {
                (AbruptExit::Break, ControlFrame::Loop { breaks, .. })
                | (AbruptExit::Break, ControlFrame::Switch { breaks, .. }) => Some(breaks),
                (AbruptExit::Continue, ControlFrame::Loop { continues, .. }) => Some(continues),
                _ => None,
            };
            if let Some(targets) = targets {
                for target in targets {
                    if target == before {
                        *target = after.clone();
                    }
                }
                return;
            }
        }
    }

    fn assign(&mut self, target: ValueId, source: ValueId) {
        if target == ValueId::UNKNOWN {
            return;
        }
        if let Some(call) = self.calls_by_result.get(&source).cloned()
            && let Some(chain) = call.chain.as_deref()
            && let Some((object, states)) = self.source_match(chain, &call.args, call.fact_id)
        {
            if self.states.len().saturating_add(states.len()) > self.limits.max_states {
                return;
            }
            self.aliases.insert(target, object);
            for state in states {
                self.states.insert((object, state.flow), state);
            }
            return;
        }
        if let Some(object) = self.aliases.get(&source).copied() {
            self.aliases.insert(target, object);
        } else {
            self.unbind_value(target);
        }
    }

    fn source_match(
        &mut self,
        chain: &str,
        args: &[CallArgInfo],
        source_fact: FactId,
    ) -> Option<(ObjectId, Vec<FlowState>)> {
        let ids = self.flow_index.sources.get(chain)?;
        let matching = ids
            .iter()
            .copied()
            .filter(|id| {
                self.flow_index.get(*id).is_some_and(|flow| {
                    flow.sources.iter().any(|source| {
                        source.member_call == chain
                            && source.arg_strings.iter().all(|matcher| {
                                args.get(matcher.index).is_some_and(|arg| {
                                    arg.static_string.as_ref().is_some_and(|value| {
                                        matcher.predicate.as_ref().map_or_else(
                                            || {
                                                matcher.values.is_empty()
                                                    || matcher.values.contains(value)
                                            },
                                            |predicate| {
                                                super::flow_calls::matches_static_value(
                                                    predicate, value,
                                                )
                                            },
                                        )
                                    })
                                })
                            })
                    })
                })
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return None;
        }
        let object = self.allocate_object_id()?;
        let states = matching
            .into_iter()
            .map(|flow| FlowState {
                flow,
                source_event: source_fact,
                object_id: object,
                requirements: BTreeMap::new(),
            })
            .collect();
        Some((object, states))
    }

    fn record_property_write(
        &mut self,
        receiver: ValueId,
        property: Option<&str>,
        value: Option<&str>,
        event: FactId,
    ) {
        let Some(object) = self.aliases.get(&receiver).copied() else {
            return;
        };
        let keys = self
            .states
            .keys()
            .filter(|(id, _)| *id == object)
            .copied()
            .collect::<Vec<_>>();
        for key in keys {
            let Some(flow) = self.flow_index.get(key.1) else {
                continue;
            };
            let Some(state) = self.states.get_mut(&key) else {
                continue;
            };
            for (index, requirement) in flow.requirements.iter().enumerate() {
                if let FlowRequirement::PropertyWrite {
                    property: expected,
                    value: matcher,
                } = requirement
                    && (property.is_none() || property == Some(expected.as_str()))
                {
                    state.requirements.remove(&index);
                    if property == Some(expected.as_str())
                        && super::flow_calls::flow_value_matches(matcher, value, true)
                    {
                        state.requirements.insert(index, event);
                    }
                }
            }
            self.emit_if_ready(key.1, key.0, event);
        }
    }

    fn record_configuration(
        &mut self,
        receiver: Option<ValueId>,
        chain: &str,
        args: &[CallArgInfo],
        event: FactId,
    ) {
        let objects = match receiver {
            Some(value) => self
                .aliases
                .get(&value)
                .copied()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            None => self.aliases.values().copied().collect::<BTreeSet<_>>(),
        };
        for object in objects {
            let keys = self
                .states
                .keys()
                .filter(|(id, _)| *id == object)
                .copied()
                .collect::<Vec<_>>();
            for key in keys {
                let Some(flow) = self.flow_index.get(key.1) else {
                    continue;
                };
                let Some(state) = self.states.get_mut(&key) else {
                    continue;
                };
                for (index, requirement) in flow.requirements.iter().enumerate() {
                    if let FlowRequirement::MemberCall {
                        member,
                        args: matchers,
                    } = requirement
                        && (member == chain || chain.rsplit('.').next() == Some(member.as_str()))
                        && matchers.iter().all(|matcher| {
                            args.get(matcher.index).is_some_and(|arg| {
                                super::flow_calls::flow_value_matches(
                                    &matcher.value,
                                    arg.static_string.as_deref(),
                                    true,
                                )
                            })
                        })
                    {
                        state.requirements.insert(index, event);
                    }
                }
                self.emit_if_ready(key.1, key.0, event);
            }
        }
    }

    fn record_sinks(&mut self, chain: &str, args: &[CallArgInfo], sink_fact: FactId) {
        let Some(flow_ids) = self.flow_index.sinks.get(chain).cloned() else {
            return;
        };
        for (argument_index, argument) in args.iter().enumerate() {
            let Some(object) = self.aliases.get(&argument.value).copied() else {
                continue;
            };
            let states = self
                .states
                .iter()
                .filter(|((id, flow), _)| *id == object && flow_ids.contains(flow))
                .map(|(_, state)| state.clone())
                .collect::<Vec<_>>();
            for state in states {
                let Some(flow) = self.flow_index.get(state.flow).cloned() else {
                    continue;
                };
                let matches = flow.sinks.iter().any(|sink| {
                    sink.member_calls.iter().any(|member| member == chain)
                        && match &sink.args {
                            FlowSinkArgs::Any => true,
                            FlowSinkArgs::Indices(indices) => indices.contains(&argument_index),
                        }
                });
                if matches {
                    self.emit_state(&state, &flow, sink_fact);
                }
            }
        }
    }

    fn record_helper_sink(
        &mut self,
        function: super::value::FunctionId,
        args: &[CallArgInfo],
        sink_fact: FactId,
    ) {
        let Some(summary) = self.helpers.get(function).cloned() else {
            return;
        };
        if !invocation_is_compatible(&summary, args) {
            return;
        }
        for sink in summary.sinks {
            let Some(parameter) = summary.parameters.iter().find(|parameter| {
                parameter.parameter_index == sink.parameter_index
                    && (parameter.path == sink.path
                        || (parameter.rest && sink.path.starts_with(&parameter.path)))
            }) else {
                continue;
            };
            let mut parameter = parameter.clone();
            parameter.path = sink.path.clone();
            let Some(value) = project_parameter_argument(args, &parameter) else {
                continue;
            };
            let Some(object) = self.aliases.get(&value).copied() else {
                continue;
            };
            let Some(state) = self.states.get(&(object, sink.flow)).cloned() else {
                continue;
            };
            let Some(flow) = self.flow_index.get(sink.flow).cloned() else {
                continue;
            };
            if state_is_ready(&state, &flow) {
                self.emit_state(&state, &flow, sink_fact);
            }
        }
    }

    fn emit_if_ready(&mut self, flow: FlowId, object: ObjectId, event: FactId) {
        let Some(state) = self.states.get(&(object, flow)).cloned() else {
            return;
        };
        let Some(matcher) = self.flow_index.get(flow).cloned() else {
            return;
        };
        if matcher.emit_on_requirements {
            self.emit_state(&state, &matcher, event);
        }
    }

    fn emit_state(&mut self, state: &FlowState, flow: &FlowMatcher, match_fact: FactId) {
        if !state_is_ready(state, flow) {
            return;
        }
        debug_assert!(state.source_event <= match_fact);
        let key = (
            state.flow.rule_index,
            state.flow.flow_index,
            state.object_id,
            match_fact,
        );
        if !self.emitted.contains(&key) && self.emitted.len() >= self.limits.max_emissions {
            return;
        }
        if self.emitted.insert(key) {
            // Requirement-only flows are anchored at the event that made the
            // final requirement true; sink flows use the sink event passed by
            // the caller. Keep the span and event identity parallel.
            let anchor = match_fact;
            self.evidence[state.flow.rule_index].push(ApiEvidence {
                kind: ApiMatchKind::CallArgument,
                symbol: flow.evidence_symbol(),
                count: 1,
                spans: vec![self.fact_spans.get(&anchor).copied().unwrap_or_default()],
                event_ids: vec![anchor.0],
            });
        }
    }

    fn unbind_value(&mut self, value: ValueId) {
        let Some(object) = self.aliases.remove(&value) else {
            return;
        };
        if !self.aliases.values().any(|alias| *alias == object) {
            self.states.retain(|(id, _), _| *id != object);
        }
    }

    fn invalidate_object(&mut self, value: ValueId) {
        let Some(object) = self.aliases.get(&value).copied() else {
            return;
        };
        self.states.retain(|(id, _), _| *id != object);
    }

    fn allocate_object_id(&mut self) -> Option<ObjectId> {
        if self.next_object_id >= self.limits.max_objects {
            return None;
        }
        let object = ObjectId(self.next_object_id);
        self.next_object_id = self.next_object_id.checked_add(1)?;
        Some(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::rule::FlowValueMatcher;
    use crate::matcher::semantic::resolver::Resolver;

    fn collect_source(source: &str, flow: &FlowMatcher) -> Vec<Vec<ApiEvidence>> {
        let parsed = crate::parse(source, "fact-flow.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let stream =
            crate::matcher::semantic::fact_builder::build_test_stream(&parsed.program, &resolver);
        collect_with_limits(&stream, &[(0, 0, flow)], 1, FlowLimits::default())
    }

    fn script_flow() -> FlowMatcher {
        FlowMatcher::new("script insertion")
            .source_member_call("document.createElement")
            .source_arg_string(0, ["script"])
            .property_write("src", FlowValueMatcher::Any)
            .sink_member_call_arg_indices(["document.head.appendChild"], [0])
    }

    #[test]
    fn transfers_source_configuration_and_sink_from_facts() {
        let evidence = collect_source(
            "const script = document.createElement('script'); script.src = url; document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn member_call_configuration_stays_with_its_receiver() {
        let flow = FlowMatcher::new("configured script")
            .source_member_call("document.createElement")
            .source_arg_string(0, ["script"])
            .member_call_config(
                "configure",
                [(0, FlowValueMatcher::StaticExact(vec!["yes".into()]))],
            )
            .sink_member_call_arg_indices(["document.head.appendChild"], [0]);
        let evidence = collect_source(
            "const first = document.createElement('script'); const second = document.createElement('script'); first.configure('yes'); document.head.appendChild(second); document.head.appendChild(first);",
            &flow,
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn property_invalidation_is_driven_by_assignment_facts() {
        let evidence = collect_source(
            "const script = document.createElement('script'); script.src = url; script.src += suffix; document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn separate_sink_facts_produce_separate_match_occurrences() {
        let evidence = collect_source(
            "const script = document.createElement('script'); script.src = url; document.head.appendChild(script); document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 2);
    }

    #[test]
    fn unchanged_branch_paths_retain_baseline_state() {
        let evidence = collect_source(
            "const script = document.createElement('script'); script.src = url; if (ready) {} document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn identical_branch_requirements_are_definite() {
        let evidence = collect_source(
            "const script = document.createElement('script'); if (ready) { script.src = url; } else { script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn one_arm_requirement_does_not_leak_after_join() {
        let evidence = collect_source(
            "const script = document.createElement('script'); if (ready) { script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn zero_iteration_loops_do_not_make_body_configuration_definite() {
        let evidence = collect_source(
            "const script = document.createElement('script'); while (ready) { script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn do_while_body_configuration_is_reachable_after_loop() {
        let evidence = collect_source(
            "const script = document.createElement('script'); do { script.src = url; } while (ready); document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn catch_only_configuration_does_not_become_definite() {
        let evidence = collect_source(
            "const script = document.createElement('script'); try { work(); } catch (error) { script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn catch_sink_can_consume_a_source_from_before_try() {
        let evidence = collect_source(
            "const script = document.createElement('script'); script.src = url; try { work(); } catch (error) { document.head.appendChild(script); }",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn finally_configuration_is_applied_to_normal_completion() {
        let evidence = collect_source(
            "const script = document.createElement('script'); try { work(); } finally { script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn switch_no_match_path_prevents_case_only_configuration() {
        let evidence = collect_source(
            "const script = document.createElement('script'); switch (kind) { case 1: script.src = url; break; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn default_case_can_make_configuration_definite() {
        let evidence = collect_source(
            "const script = document.createElement('script'); switch (kind) { case 1: script.src = url; break; default: script.src = url; } document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn do_while_break_preserves_the_break_exit() {
        let evidence = collect_source(
            "const script = document.createElement('script'); do { script.src = url; break; } while (ready); document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn finally_configuration_reaches_a_break_exit() {
        let evidence = collect_source(
            "const script = document.createElement('script'); do { try { break; } finally { script.src = url; } } while (ready); document.head.appendChild(script);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn finally_return_does_not_reach_code_after_the_try() {
        let evidence = collect_source(
            "function run() { const script = document.createElement('script'); try { return; } finally { script.src = url; } document.head.appendChild(script); }",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn destructuring_assignment_invalidates_the_written_alias() {
        let evidence = collect_source(
            "let script = document.createElement('script'); script.src = url; ({ script } = replacement); document.head.appendChild(script);",
            &script_flow(),
        );
        assert!(evidence[0].is_empty());
    }

    #[test]
    fn rebinding_one_alias_does_not_kill_the_shared_object() {
        let evidence = collect_source(
            "let first = document.createElement('script'); const alias = first; first = replacement; alias.src = url; document.head.appendChild(alias);",
            &script_flow(),
        );
        assert_eq!(evidence[0].iter().map(|item| item.count).sum::<u32>(), 1);
    }

    #[test]
    fn flow_evidence_is_anchored_at_the_sink_event() {
        let source = "const script = document.createElement('script'); script.src = url; document.head.appendChild(script);";
        let parsed = crate::parse(source, "flow-location.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let stream =
            crate::matcher::semantic::fact_builder::build_test_stream(&parsed.program, &resolver);
        let sink_span = stream
            .facts()
            .iter()
            .find_map(|fact| match &fact.payload {
                FactPayload::Call {
                    syntactic_chain: Some(chain),
                    ..
                } if chain == "document.head.appendChild" => Some(fact.span),
                _ => None,
            })
            .expect("sink call should be present");
        let evidence =
            collect_with_limits(&stream, &[(0, 0, &script_flow())], 1, FlowLimits::default());
        assert_eq!(evidence[0][0].spans, vec![sink_span]);
    }

    #[test]
    fn requirement_only_evidence_is_anchored_at_the_configuration_event() {
        let flow = FlowMatcher::new("configured input")
            .source_member_call("document.createElement")
            .source_arg_string(0, ["input"])
            .property_write("type", FlowValueMatcher::StaticExact(vec!["file".into()]))
            .emit_when_requirements_met();
        let source = "const input = document.createElement('input'); input.type = 'file';";
        let parsed =
            crate::parse(source, "flow-requirement-location.js").expect("source should parse");
        let resolver = Resolver::collect(&parsed.program);
        let stream =
            crate::matcher::semantic::fact_builder::build_test_stream(&parsed.program, &resolver);
        let configuration = stream
            .facts()
            .iter()
            .find_map(|fact| {
                matches!(fact.payload, FactPayload::PropertyWrite { .. })
                    .then_some((fact.id, fact.span))
            })
            .expect("configuration write should be present");
        let evidence = collect_with_limits(&stream, &[(0, 0, &flow)], 1, FlowLimits::default());
        assert_eq!(evidence[0][0].spans, vec![configuration.1]);
        assert_eq!(evidence[0][0].event_ids, vec![configuration.0.0]);
    }
}
