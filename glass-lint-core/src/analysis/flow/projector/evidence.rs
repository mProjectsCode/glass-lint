//! Evidence emission and flow requirement updates.
//!
//! Configuration events update only the object states reachable through proven
//! aliases. Emissions are anchored at the event that completed the flow and
//! deduplicated by flow/object/event before the bounded result is returned.

use crate::{
    analysis::{
        facts::FactStream,
        flow::{
            index::{FlowId, FlowLimits},
            plan::BoundFlowPlan,
            projector::{
                CallArgInfo, ClassificationEvidence, CompiledObjectFlow, FactId, FlowState,
                MatchKind, ObjectFlowProjector, ObjectId, ValueId,
                state::{FlowEvidence, ReportEvidenceKey},
            },
            state::FlowStateKey,
            summary::SummaryPathStore,
        },
        value::NamePath,
    },
    api::compiler::{CompiledObjectRequirement, CompiledObjectSinkArguments},
};

impl ObjectFlowProjector<'_, '_> {
    /// Apply member-call requirements to live object states.
    pub(super) fn record_configuration(
        &mut self,
        receiver: Option<ValueId>,
        chain: &NamePath,
        args: &[CallArgInfo],
        event: FactId,
    ) {
        let objects: Vec<ObjectId> = match receiver {
            Some(value) => self.flow_state.object_for(value).into_iter().collect(),
            None => self.flow_state.objects().collect(),
        };
        for object in objects {
            let keys: Vec<_> = self
                .flow_state
                .states_for(object)
                .map(|(key, _)| key)
                .collect();
            for key in keys {
                let Some(flow) = self.plan.get(key.flow) else {
                    continue;
                };
                let Some(mut state) = self.flow_state.state_mut(key.object, key.flow) else {
                    continue;
                };
                let req_members = self.plan.requirement_members(key.flow);
                for (index, member) in req_members.iter().enumerate() {
                    if let Some(member) = member
                        && (member == chain || chain.last_segment() == member.last_segment())
                        && let CompiledObjectRequirement::MemberCall {
                            arguments: matchers,
                            ..
                        } = &flow.requirements[index]
                        && matchers.iter().all(|matcher| {
                            args.get(matcher.index()).is_some_and(|arg| {
                                match self.stream.values() {
                                    Some(values) => {
                                        matcher.matcher().matches(arg, self.names, values)
                                    }
                                    None => false,
                                }
                            })
                        })
                    {
                        state.record_requirement(index, event);
                    }
                }
                drop(state);
                emit_if_ready(
                    &mut self.flow_evidence,
                    &self.flow_state,
                    &self.plan,
                    &self.limits,
                    self.stream,
                    key.flow,
                    key.object,
                    event,
                );
            }
        }
    }

    /// Check sink arguments against live states and emit completed flows.
    pub(super) fn record_sinks(
        &mut self,
        chain: &NamePath,
        args: &[CallArgInfo],
        sink_fact: FactId,
        rooted: bool,
    ) {
        let Some(flow_ids) = self.plan.sink_ids(chain) else {
            return;
        };
        for (argument_index, argument) in args.iter().enumerate() {
            let Some(object) = self.flow_state.object_for(argument.value) else {
                continue;
            };
            let pairs: Vec<(FlowStateKey, FlowId)> = self
                .flow_state
                .states_for(object)
                .filter(|(key, _)| flow_ids.contains(&key.flow))
                .map(|(key, _)| (key, key.flow))
                .collect();
            for (key, flow_id) in pairs {
                let Some(flow) = self.plan.get(flow_id) else {
                    continue;
                };
                let sink_members = self.plan.sink_member_calls(flow_id);
                let matches = flow.sinks.iter().enumerate().any(|(i, sink)| {
                    sink_members
                        .get(i)
                        .is_some_and(|members| members.iter().any(|member| member == chain))
                        && sink.is_rooted == rooted
                        && match &sink.args {
                            CompiledObjectSinkArguments::Any => true,
                            CompiledObjectSinkArguments::Indices(indices) => {
                                indices.contains(&argument_index)
                            }
                        }
                });
                if matches {
                    let Some(state) = self.flow_state.state(key.object, key.flow) else {
                        continue;
                    };
                    emit_state(
                        &mut self.flow_evidence,
                        self.stream,
                        &self.limits,
                        state,
                        flow,
                        sink_fact,
                    );
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
        let Some(summary) = self.helpers.get(function) else {
            return;
        };
        if !summary.is_invocation_compatible(self.stream, args, self.helpers.path_interner()) {
            return;
        }
        let paths = self.helpers.path_interner();
        let ready: Vec<(ObjectId, FlowId)> = summary
            .sinks()
            .into_iter()
            .filter_map(|sink| {
                let parameter =
                    summary
                        .parameter_bindings(self.stream)
                        .iter()
                        .find(|parameter| {
                            parameter.parameter_index == sink.parameter_index()
                                && (SummaryPathStore::matches_frozen(sink.path(), parameter.path)
                                    || (parameter.rest
                                        && paths.starts_with_frozen(sink.path(), parameter.path)))
                        })?;
                let value = parameter.project_argument_at(self.stream, args, paths, sink.path())?;
                let object = self.flow_state.object_for(value)?;
                let state = self.flow_state.state(object, sink.flow())?;
                let flow = self.plan.get(sink.flow())?;
                if state.is_ready(flow) {
                    Some((object, sink.flow()))
                } else {
                    None
                }
            })
            .collect();
        for (object, flow_id) in ready {
            let Some(state) = self.flow_state.state(object, flow_id) else {
                continue;
            };
            let Some(flow) = self.plan.get(flow_id) else {
                continue;
            };
            emit_state(
                &mut self.flow_evidence,
                self.stream,
                &self.limits,
                state,
                flow,
                sink_fact,
            );
        }
    }
}

/// Emit a requirement-only match when its state is complete.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_if_ready(
    evidence: &mut FlowEvidence,
    flow_state: &super::state::FlowStateTable,
    plan: &BoundFlowPlan<'_>,
    limits: &FlowLimits,
    stream: &FactStream,
    flow: FlowId,
    object: ObjectId,
    event: FactId,
) {
    let Some(state) = flow_state.state(object, flow) else {
        return;
    };
    let Some(matcher) = plan.get(flow) else {
        return;
    };
    if matcher.emit_on_requirements {
        emit_state(evidence, stream, limits, state, matcher, event);
    }
}

/// Emit one bounded, source-anchored evidence item for a ready state.
fn emit_state(
    evidence: &mut FlowEvidence,
    stream: &FactStream,
    limits: &FlowLimits,
    state: &FlowState,
    flow: &CompiledObjectFlow,
    match_fact: FactId,
) {
    if !state.is_ready(flow) {
        return;
    }
    debug_assert!(state.source_event() <= match_fact);
    let key = ReportEvidenceKey::new(
        state.flow_id().rule_index().get(),
        state.flow_id().flow_index(),
        state.object_id(),
        match_fact,
    );
    if evidence.try_insert(key, limits.emission_limit()) {
        let anchor = match_fact;
        evidence.record(
            state.flow_id().rule_index().get(),
            ClassificationEvidence {
                kind: MatchKind::CallArgument,
                symbol: flow.evidence_symbol(),
                count: 1,
                evidence_truncated: false,
                occurrences: vec![
                    crate::api::classification::ClassificationEvidenceOccurrence {
                        span: stream
                            .fact(anchor)
                            .map_or(crate::ByteRange::empty(), |fact| fact.span),
                        fact: Some(anchor.0),
                    },
                ],
                related: Vec::new(),
            },
        );
    }
}
