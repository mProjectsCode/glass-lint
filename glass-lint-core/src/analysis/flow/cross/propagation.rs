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
            planning::BoundFlowPlan,
        },
    },
    api::compiler::{CompiledObjectFlow, CompiledObjectRequirement, object_flow::CompletionMode},
    project::ModuleId,
};

pub(super) struct UsageProjector<'a, 'session> {
    pub(super) session: &'a mut CrossProjectionSession<'session>,
    pub(super) context: &'a CallContext,
    pub(super) effect: &'a FunctionEffect,
    pub(super) flow: &'a CompiledObjectFlow,
    pub(super) flow_plan: &'a BoundFlowPlan<'session>,
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
                module: self.context.module(),
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
            .module_fact_stream(self.context.module())
            .and_then(|stream| {
                let value = stream.property_write_value(event)?;
                stream.values().static_string(value)
            });
        let mut next = self.state.clone();
        for (index, requirement) in self
            .flow_plan
            .requirements_with_indices(self.context.state().flow_id())
        {
            if let crate::api::compiler::CompiledObjectRequirement::PropertyWrite {
                property: expected,
                value,
            } = requirement
                && property == Some(expected)
                && value_is_precise
                && value.matches_flow_value(static_value)
            {
                next.record_requirement(index, QualifiedEvent::new(self.context.module(), event));
            }
        }
        self.emit_requirements(&next, event);
        *self.state = next;
    }

    fn apply_receiver(&mut self, event: FactId) {
        let Some(stream) = self
            .session
            .project
            .module_fact_stream(self.context.module())
        else {
            return;
        };
        let cref = CallEffectRef { stream, event };
        let Some(call_args) = cref.effective_args() else {
            return;
        };

        let chain = cref.chain();
        let values = stream.values();
        let mut next = self.state.clone();
        for (index, member, requirement) in self
            .flow_plan
            .member_requirements(self.context.state().flow_id())
        {
            if chain.is_some_and(|c| c == member || c.last_segment() == member.last_segment())
                && let CompiledObjectRequirement::MemberCall { arguments, .. } = requirement
                && arguments.iter().all(|matcher| {
                    call_args.get(matcher.index()).is_some_and(|argument| {
                        matcher
                            .predicate()
                            .matches(argument, self.session.names, values)
                    })
                })
            {
                next.record_requirement(index, QualifiedEvent::new(self.context.module(), event));
            }
        }
        self.emit_requirements(&next, event);
        *self.state = next;
    }

    fn apply_argument(&mut self, event: FactId, argument: usize) {
        let Some(stream) = self
            .session
            .project
            .module_fact_stream(self.context.module())
        else {
            return;
        };
        let cref = CallEffectRef { stream, event };
        let matching_sinks = self.flow_plan.matching_sink_indices(
            self.context.state().flow_id(),
            argument,
            |target| cref.matches_target(target, self.session.names),
        );
        if !matching_sinks.is_empty() && self.context.is_crossed() {
            for index in matching_sinks {
                self.state
                    .record_sink(index, QualifiedEvent::new(self.context.module(), event));
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
                    self.context.module(),
                    self.context.state().flow_id(),
                    self.state,
                    event,
                    self.flow,
                );
            } else {
                mark_nonmatching(
                    self.session.evidence,
                    self.context.module(),
                    self.context.state().flow_id(),
                    event,
                    self.flow,
                );
            }
        }
    }

    fn emit_requirements(&mut self, state: &CrossFlowState, event: FactId) {
        if self.flow.completion_mode == CompletionMode::Configuration
            && state.requirements_ready(self.flow)
            && self.context.is_crossed()
        {
            emit(
                evidence::EmissionContext {
                    project: self.session.project,
                    evidence: self.session.evidence,
                    arena: self.session.arena,
                },
                self.context.module(),
                self.context.state().flow_id(),
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
                    self.context.matches_parameter(
                        parameter.index(),
                        parameter.is_root(),
                        argument.is_root(),
                    )
                }) || self.context.matches_source_root(
                    self.effect
                        .value_root(argument.value())
                        .unwrap_or_else(|| argument.value()),
                    argument.is_root(),
                    true,
                );
                if connected {
                    self.session.worklist.enqueue_parameters(
                        self.session.project,
                        target_module,
                        target_function,
                        argument.index(),
                        self.state,
                        self.context.is_crossed() || target_module != self.module,
                    );
                }
            }
        }
    }
}
