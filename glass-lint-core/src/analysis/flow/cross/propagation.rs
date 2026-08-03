use std::collections::BTreeSet;

use smol_str::SmolStr;

use super::CrossProjectionSession;
use crate::{
    analysis::{
        facts::FactId,
        flow::{
            cross::{
                evidence::{self, effect_use_event, emit, mark_nonmatching, usage_matches_context},
                state::{CallContext, CrossFlowState, QualifiedEvent},
            },
            effect::{CallEffectRef, EffectUse, FunctionEffect},
            planning::BoundFlowPaths,
        },
        model::flow::{RequirementIndex, SinkIndex},
    },
    api::compiler::{
        CompiledObjectFlow, CompiledObjectRequirement, CompiledObjectSinkArguments,
        object_flow::CompletionMode,
    },
    project::ModuleId,
};

pub(super) struct UsageProjector<'a, 'session> {
    pub(super) session: &'a mut CrossProjectionSession<'session>,
    pub(super) context: &'a CallContext,
    pub(super) effect: &'a FunctionEffect,
    pub(super) flow: &'a CompiledObjectFlow,
    pub(super) flow_plan: &'a BoundFlowPaths,
    pub(super) state: &'a mut CrossFlowState,
    pub(super) propagated: &'a mut BTreeSet<FactId>,
}

impl UsageProjector<'_, '_> {
    pub(super) fn project(&mut self) {
        for usage in self.effect.uses() {
            if !usage_matches_context(self.effect, usage, self.context) {
                continue;
            }
            CallPropagation {
                session: self.session,
                effect: self.effect,
                module: self.context.module,
                context: self.context,
                propagated: self.propagated,
                through: Some(effect_use_event(usage)),
                state: self.state,
            }
            .propagate();
            match usage {
                EffectUse::PropertyWrite {
                    event,
                    property,
                    value_is_precise,
                    ..
                } => self.apply_property(*event, property.as_ref(), *value_is_precise),
                EffectUse::CallReceiver { event, .. } => {
                    self.apply_receiver(*event);
                }
                EffectUse::CallArgument {
                    event,
                    argument_index,
                    ..
                } => {
                    self.apply_argument(*event, *argument_index);
                }
            }
        }
    }

    fn apply_property(
        &mut self,
        event: FactId,
        property: Option<&SmolStr>,
        value_is_precise: bool,
    ) {
        let static_value = self
            .session
            .project
            .module_fact_stream(self.context.module)
            .and_then(|stream| {
                let value = stream.property_write_value(event)?;
                stream.values().static_string(value)
            });
        let mut next = self.state.clone();
        for (index, requirement) in self.flow.requirements.iter().enumerate() {
            if let crate::api::compiler::CompiledObjectRequirement::PropertyWrite {
                property: expected,
                value,
            } = requirement
                && property == Some(expected)
                && value_is_precise
                && value.matches_flow_value(static_value)
            {
                next.record_requirement(
                    RequirementIndex::new(index),
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

    fn apply_receiver(&mut self, event: FactId) {
        let Some(stream) = self.session.project.module_fact_stream(self.context.module) else {
            return;
        };
        let cref = CallEffectRef { stream, event };
        let Some(call_args) = cref.effective_args() else {
            return;
        };

        let chain = cref.chain();
        let values = stream.values();
        let mut next = self.state.clone();
        for (index, member) in self.flow_plan.requirement_members().iter().enumerate() {
            if let Some(member) = member
                && chain.is_some_and(|c| c == member || c.last_segment() == member.last_segment())
                && let CompiledObjectRequirement::MemberCall { arguments, .. } =
                    &self.flow.requirements[index]
                && arguments.iter().all(|matcher| {
                    call_args.get(matcher.index()).is_some_and(|argument| {
                        matcher
                            .predicate()
                            .matches(argument, self.session.names, values)
                    })
                })
            {
                next.record_requirement(
                    RequirementIndex::new(index),
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

    fn apply_argument(&mut self, event: FactId, argument: usize) {
        let Some(stream) = self.session.project.module_fact_stream(self.context.module) else {
            return;
        };
        let cref = CallEffectRef { stream, event };
        let matching_sinks: Vec<usize> = self
            .flow
            .sinks
            .iter()
            .enumerate()
            .filter_map(|(i, sink)| {
                let matches = cref.matches_target(&sink.target, self.session.names)
                    && match &sink.args {
                        CompiledObjectSinkArguments::Any => true,
                        CompiledObjectSinkArguments::Indices(indices) => {
                            indices.contains(&argument)
                        }
                    };
                matches.then_some(i)
            })
            .collect();
        if !matching_sinks.is_empty() && self.context.crossed {
            for index in matching_sinks {
                self.state.record_sink(
                    SinkIndex::new(index),
                    QualifiedEvent {
                        module: self.context.module,
                        fact: event,
                    },
                );
            }
            if self.state.requirements_ready(self.flow)
                && self.state.source().is_some()
                && self.state.sinks_complete(self.flow)
            {
                emit(
                    evidence::EmissionContext {
                        project: self.session.project,
                        evidence: self.session.evidence,
                        arena: self.session.arena,
                    },
                    self.context.module,
                    self.context.state.flow_id(),
                    self.state,
                    event,
                    self.flow,
                );
            } else {
                mark_nonmatching(
                    self.session.evidence,
                    self.context.module,
                    self.context.state.flow_id(),
                    event,
                    self.flow,
                );
            }
        }
    }

    fn emit_requirements(&mut self, state: &CrossFlowState, event: FactId) {
        if self.flow.completion_mode == CompletionMode::Configuration
            && state.requirements_ready(self.flow)
            && self.context.crossed
        {
            emit(
                evidence::EmissionContext {
                    project: self.session.project,
                    evidence: self.session.evidence,
                    arena: self.session.arena,
                },
                self.context.module,
                self.context.state.flow_id(),
                state,
                event,
                self.flow,
            );
        }
    }
}

pub(super) struct CallPropagation<'a, 'session> {
    pub(super) session: &'a mut CrossProjectionSession<'session>,
    pub(super) effect: &'a FunctionEffect,
    pub(super) module: ModuleId,
    pub(super) context: &'a CallContext,
    pub(super) propagated: &'a mut BTreeSet<FactId>,
    pub(super) through: Option<FactId>,
    pub(super) state: &'a CrossFlowState,
}

impl CallPropagation<'_, '_> {
    pub(super) fn propagate(&mut self) {
        for call in self.effect.calls() {
            if self.through.is_some_and(|event| call.event() > event)
                || !self.propagated.insert(call.event())
            {
                continue;
            }
            let Some((target_module, target_function)) =
                self.session.call_graph.get(self.module, call.event())
            else {
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
                    self.session.worklist.enqueue_parameters(
                        self.session.project,
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
