//! Evidence emission and flow requirement updates.

use super::{
    ApiEvidence, ApiMatchKind, BTreeSet, CallArgInfo, CompiledObjectFlow, FactId, FlowId,
    FlowState, ObjectFlowProjector, ObjectId, ValueId,
};
use crate::api::compiler::{CompiledObjectRequirement, CompiledObjectSinkArgs};

impl ObjectFlowProjector<'_, '_> {
    pub(super) fn record_configuration(
        &mut self,
        receiver: Option<ValueId>,
        chain: &str,
        args: &[CallArgInfo],
        event: FactId,
    ) {
        // A missing receiver represents a call through a helper summary or a
        // rooted operation whose object identity is not available. In that
        // case conservatively try every live object, while receiver-bearing
        // calls stay scoped to their proven alias.
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
                    if let CompiledObjectRequirement::MemberCall {
                        member,
                        arguments: matchers,
                    } = requirement
                        && (member == chain || chain.rsplit('.').next() == Some(member.as_str()))
                        && matchers.iter().all(|matcher| {
                            args.get(matcher.index)
                                .is_some_and(|arg| matcher.matcher.matches(arg))
                        })
                    {
                        state.requirements.insert(index, event);
                    }
                }
                self.emit_if_ready(key.1, key.0, event);
            }
        }
    }

    pub(super) fn record_sinks(
        &mut self,
        chain: &str,
        args: &[CallArgInfo],
        sink_fact: FactId,
        rooted: bool,
    ) {
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
                        && sink.provenance.matches_rooted(rooted)
                        && match &sink.args {
                            CompiledObjectSinkArgs::Any => true,
                            CompiledObjectSinkArgs::Indices(indices) => {
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
        for sink in summary.sinks {
            let Some(parameter) = summary.parameters.iter().find(|parameter| {
                parameter.parameter_index == sink.parameter_index
                    && (parameter.path == sink.path
                        || (parameter.rest
                            && self.stream.paths().starts_with(sink.path, parameter.path)))
            }) else {
                continue;
            };
            let mut parameter = parameter.clone();
            parameter.path = sink.path;
            let Some(value) = parameter.project_argument(self.stream, args) else {
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
            if state.is_ready(&flow) {
                self.emit_state(&state, &flow, sink_fact);
            }
        }
    }

    pub(super) fn emit_if_ready(&mut self, flow: FlowId, object: ObjectId, event: FactId) {
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

    pub(super) fn emit_state(
        &mut self,
        state: &FlowState,
        flow: &CompiledObjectFlow,
        match_fact: FactId,
    ) {
        if !state.is_ready(flow) {
            return;
        }
        debug_assert!(state.source_event <= match_fact);
        let key = (
            state.flow.rule_index,
            state.flow.flow_index,
            state.object_id,
            match_fact,
        );
        if !self.emitted.contains(&key) && self.emitted.len() >= self.limits.emissions {
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
                related: Vec::new(),
            });
        }
    }
}
