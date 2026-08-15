use std::collections::BTreeSet;

use smol_str::SmolStr;

use super::CrossProjectionSession;
use crate::{
    analysis::{
        facts::{FactId, FactStream, Frozen},
        flow::{
            cross::{
                evidence::{emit, mark_nonmatching, usage_matches_context},
                state::{CallContext, CrossFlowState, EvidenceTransition},
            },
            effect::{EffectUse, FunctionEffect},
            planning::{BoundFlowPlan, FlowMatchView, PropertyRequirementMatch},
        },
        model::flow::RequirementIndex,
        trace::QualifiedEvent,
    },
    api::compiler::{CompiledObjectFlow, object_flow::CompletionMode},
};

pub(super) struct UsageProjector<'a, 'session> {
    session: &'a mut CrossProjectionSession<'session>,
    context: &'a CallContext,
    effect: &'a FunctionEffect,
    flow: &'a CompiledObjectFlow,
    flow_plan: &'a BoundFlowPlan<'session>,
    state: &'a mut CrossFlowState,
    propagated: &'a mut BTreeSet<FactId>,
    stream: &'a FactStream<Frozen>,
    matcher: FlowMatchView<'a>,
}

impl UsageProjector<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new<'a, 'session>(
        session: &'a mut CrossProjectionSession<'session>,
        context: &'a CallContext,
        effect: &'a FunctionEffect,
        flow: &'a CompiledObjectFlow,
        flow_plan: &'a BoundFlowPlan<'session>,
        state: &'a mut CrossFlowState,
        propagated: &'a mut BTreeSet<FactId>,
        stream: &'a FactStream<Frozen>,
    ) -> UsageProjector<'a, 'session> {
        let matcher = FlowMatchView::new(session.names, stream.values());
        UsageProjector {
            session,
            context,
            effect,
            flow,
            flow_plan,
            state,
            propagated,
            stream,
            matcher,
        }
    }

    pub(super) fn project(&mut self) {
        for usage in self.effect.uses() {
            let event = usage.event();
            if !usage_matches_context(self.effect, usage, self.context) {
                continue;
            }
            CallPropagation::new(
                self.session,
                self.effect,
                self.context,
                self.propagated,
                Some(event),
                self.state,
            )
            .propagate();
            match usage {
                EffectUse::PropertyWrite {
                    property,
                    value_is_precise,
                    ..
                } => self.apply_property(event, property.as_ref(), *value_is_precise),
                EffectUse::CallReceiver { .. } => {
                    self.apply_receiver(event);
                }
                EffectUse::CallArgument { argument_index, .. } => {
                    self.apply_argument(event, *argument_index);
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
            .stream
            .property_write_value(event)
            .and_then(|value| self.stream.values().static_string(value));
        let matching = self
            .flow_plan
            .matching_property_requirements(
                self.context.state().flow_id(),
                property.map(SmolStr::as_str),
                static_value,
                value_is_precise,
            )
            .into_iter()
            .filter(|match_result| match_result.value_matches())
            .map(PropertyRequirementMatch::index)
            .collect();
        self.advance_requirements(event, matching);
    }

    fn apply_receiver(&mut self, event: FactId) {
        let cref = self.stream.call_effect(event);
        let Some(shape) = cref.shape() else {
            return;
        };
        let call_args = shape.effective_args();

        let chain = shape.chain();
        let matching = self.flow_plan.matching_member_requirement_indices(
            self.context.state().flow_id(),
            chain,
            call_args,
            &self.matcher,
        );
        self.advance_requirements(event, matching);
    }

    fn advance_requirements(&mut self, event: FactId, indices: Vec<RequirementIndex>) {
        let mut next = self.state.clone();
        let readiness = self.flow.readiness();
        let mut transition = next.requirement_transition(readiness);
        for index in indices {
            transition = transition.merge(next.advance_requirement(
                index,
                QualifiedEvent::new(self.context.module(), event),
                readiness,
            ));
        }
        self.emit_requirements(&next, event, transition);
        *self.state = next;
    }

    fn apply_argument(&mut self, event: FactId, argument: usize) {
        let cref = self.stream.call_effect(event);
        let Some(shape) = cref.shape() else {
            return;
        };
        let candidates = self.flow_plan.sink_candidates_for_call(&shape);
        let matching_sinks: Vec<_> = candidates
            .into_iter()
            .flatten()
            .filter(|sink| {
                sink.flow_id() == self.context.state().flow_id() && sink.matches_argument(argument)
            })
            .map(crate::analysis::flow::planning::BoundSink::index)
            .collect();
        if !matching_sinks.is_empty() && self.context.is_crossed() {
            let readiness = self.flow.readiness();
            let mut transition = self.state.sink_transition(readiness);
            for index in matching_sinks {
                transition = transition.merge(self.state.advance_sink(
                    index,
                    QualifiedEvent::new(self.context.module(), event),
                    readiness,
                ));
            }
            if transition.is_ready() {
                emit(
                    self.session,
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

    fn emit_requirements(
        &mut self,
        state: &CrossFlowState,
        event: FactId,
        transition: EvidenceTransition,
    ) {
        if self.flow.completion_mode() == CompletionMode::Configuration
            && transition.is_ready()
            && self.context.is_crossed()
        {
            emit(
                self.session,
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
    session: &'a mut CrossProjectionSession<'session>,
    effect: &'a FunctionEffect,
    context: &'a CallContext,
    propagated: &'a mut BTreeSet<FactId>,
    through: Option<FactId>,
    state: &'a CrossFlowState,
}

impl CallPropagation<'_, '_> {
    pub(super) fn new<'a, 'session>(
        session: &'a mut CrossProjectionSession<'session>,
        effect: &'a FunctionEffect,
        context: &'a CallContext,
        propagated: &'a mut BTreeSet<FactId>,
        through: Option<FactId>,
        state: &'a CrossFlowState,
    ) -> CallPropagation<'a, 'session> {
        CallPropagation {
            session,
            effect,
            context,
            propagated,
            through,
            state,
        }
    }

    pub(super) fn propagate(&mut self) {
        for call in self.effect.calls() {
            if self.through.is_some_and(|event| call.event() > event)
                || !self.propagated.insert(call.event())
            {
                continue;
            }
            let Some(target) = self
                .session
                .call_graph
                .get(QualifiedEvent::new(self.context.module(), call.event()))
            else {
                continue;
            };
            for argument in call.arguments() {
                if self.context.matches_argument(self.effect, argument) {
                    self.session.worklist.enqueue_parameters(
                        self.session.project,
                        target.module(),
                        target.function(),
                        argument.index(),
                        self.state,
                        self.context.is_crossed() || target.module() != self.context.module(),
                    );
                }
            }
        }
    }
}
