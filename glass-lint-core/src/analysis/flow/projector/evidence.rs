//! Evidence emission and flow requirement updates.
//!
//! Configuration events update only the object states reachable through proven
//! aliases. Emissions are anchored at the event that completed the flow and
//! deduplicated by flow/object/event before the bounded result is returned.

use std::collections::BTreeSet;

use super::{
    CallArgInfo, ClassificationEvidence, CompiledObjectFlow, FactId, FlowId, FlowState, MatchKind,
    ObjectFlowProjector, ObjectId, ValueId,
};
use crate::{
    analysis::SymbolPath,
    api::compiler::{CompiledObjectRequirement, CompiledObjectSinkArguments},
};

impl ObjectFlowProjector<'_, '_> {
    /// Apply member-call requirements to live object states.
    pub(super) fn record_configuration(
        &mut self,
        receiver: Option<ValueId>,
        chain: &SymbolPath,
        args: &[CallArgInfo],
        event: FactId,
    ) {
        // A missing receiver represents a call through a helper summary or a
        // rooted operation whose object identity is not available. In that
        // case conservatively try every live object, while receiver-bearing
        // calls stay scoped to their proven alias.
        let objects = match receiver {
            Some(value) => self
                .flow_state
                .object_for(value)
                .into_iter()
                .collect::<BTreeSet<_>>(),
            None => self.flow_state.objects().collect::<BTreeSet<_>>(),
        };
        for object in objects {
            let keys = self
                .flow_state
                .states_for(object)
                .map(|(key, _)| key)
                .collect::<Vec<_>>();
            for key in keys {
                let Some(flow) = self.flow_index.get(key.flow) else {
                    continue;
                };
                let Some(state) = self.flow_state.state_mut(key.object, key.flow) else {
                    continue;
                };
                for (index, requirement) in flow.requirements.iter().enumerate() {
                    if let CompiledObjectRequirement::MemberCall {
                        member,
                        arguments: matchers,
                    } = requirement
                        && (member == chain || chain.last_segment() == member.last_segment())
                        && matchers.iter().all(|matcher| {
                            args.get(matcher.index)
                                .is_some_and(|arg| matcher.matcher.matches(arg))
                        })
                    {
                        state.record_requirement(index, event);
                    }
                }
                self.emit_if_ready(key.flow, key.object, event);
            }
        }
    }

    /// Check sink arguments against live states and emit completed flows.
    pub(super) fn record_sinks(
        &mut self,
        chain: &SymbolPath,
        args: &[CallArgInfo],
        sink_fact: FactId,
        rooted: bool,
    ) {
        let Some(flow_ids) = self.flow_index.sink_ids(chain).map(<[FlowId]>::to_vec) else {
            return;
        };
        for (argument_index, argument) in args.iter().enumerate() {
            let Some(object) = self.flow_state.object_for(argument.value) else {
                continue;
            };
            let states = self
                .flow_state
                .states_for(object)
                .filter(|(key, _)| flow_ids.contains(&key.flow))
                .map(|(_, state)| state.clone())
                .collect::<Vec<_>>();
            for state in states {
                let Some(flow) = self.flow_index.get(state.flow_id()).cloned() else {
                    continue;
                };
                let matches = flow.sinks.iter().any(|sink| {
                    sink.member_calls.iter().any(|member| member == chain)
                        && sink.provenance.matches_rooted(rooted)
                        && match &sink.args {
                            CompiledObjectSinkArguments::Any => true,
                            CompiledObjectSinkArguments::Indices(indices) => {
                                indices.contains(&argument_index)
                            }
                        }
                });
                if matches {
                    self.emit_state(&state, &flow, sink_fact);
                }
            }
        }
    }

    /// Project a summarized helper sink through a concrete invocation.
    pub(super) fn record_helper_sink(
        &mut self,
        function: crate::analysis::value::FunctionId,
        args: &[CallArgInfo],
        sink_fact: FactId,
    ) {
        let Some(summary) = self.helpers.get(function).cloned() else {
            return;
        };
        if !summary.is_invocation_compatible(self.stream, args) {
            return;
        }
        for sink in summary.sinks() {
            let Some(parameter) = summary.parameters().iter().find(|parameter| {
                parameter.parameter_index == sink.parameter_index()
                    && (sink
                        .path()
                        .matches_base(self.stream.paths(), parameter.path)
                        || (parameter.rest
                            && sink
                                .path()
                                .starts_with_base(self.stream.paths(), parameter.path)))
            }) else {
                continue;
            };
            let Some(value) = parameter.project_argument_at(self.stream, args, sink.path()) else {
                continue;
            };
            let Some(object) = self.flow_state.object_for(value) else {
                continue;
            };
            let Some(state) = self.flow_state.state(object, sink.flow()).cloned() else {
                continue;
            };
            let Some(flow) = self.flow_index.get(sink.flow()).cloned() else {
                continue;
            };
            if state.is_ready(&flow) {
                self.emit_state(&state, &flow, sink_fact);
            }
        }
    }

    /// Emit a requirement-only match when its state is complete.
    pub(super) fn emit_if_ready(&mut self, flow: FlowId, object: ObjectId, event: FactId) {
        let Some(state) = self.flow_state.state(object, flow).cloned() else {
            return;
        };
        let Some(matcher) = self.flow_index.get(flow).cloned() else {
            return;
        };
        if matcher.emit_on_requirements {
            self.emit_state(&state, &matcher, event);
        }
    }

    /// Emit one bounded, source-anchored evidence item for a ready state.
    pub(super) fn emit_state(
        &mut self,
        state: &FlowState,
        flow: &CompiledObjectFlow,
        match_fact: FactId,
    ) {
        if !state.is_ready(flow) {
            return;
        }
        debug_assert!(state.source_event() <= match_fact);
        let key = super::state::ReportEvidenceKey::new(
            state.flow_id().rule_index().get(),
            state.flow_id().flow_index(),
            state.object_id(),
            match_fact,
        );
        if self
            .flow_evidence
            .try_insert(key, self.limits.emission_limit())
        {
            // Requirement-only flows are anchored at the event that made the
            // final requirement true; sink flows use the sink event passed by
            // the caller. Keep the span and event identity parallel.
            let anchor = match_fact;
            self.flow_evidence.record(
                state.flow_id().rule_index().get(),
                ClassificationEvidence {
                    kind: MatchKind::CallArgument,
                    symbol: flow.evidence_symbol(),
                    count: 1,
                    evidence_truncated: false,
                    occurrences: vec![
                        crate::api::classification::ClassificationEvidenceOccurrence {
                            span: self.fact_spans.get(&anchor).copied().unwrap_or_default(),
                            fact: Some(anchor.0),
                        },
                    ],
                    related: Vec::new(),
                },
            );
        }
    }
}
