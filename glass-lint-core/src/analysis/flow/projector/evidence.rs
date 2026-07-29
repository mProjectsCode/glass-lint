//! Evidence emission and flow requirement updates.
//!
//! Configuration events update only the object states reachable through proven
//! aliases. Emissions are anchored at the event that completed the flow and
//! deduplicated by flow/object/event before the bounded result is returned.

use glass_lint_datastructures::NamePath;

use crate::{
    analysis::{
        flow::{
            projector::{
                CallArgInfo, ClassificationEvidence, FactId, FlowState, MatchKind,
                ObjectFlowProjector, ObjectId, ValueId, history::ReportEvidenceKey,
            },
            summary::SummaryPathStore,
        },
        model::flow::{FlowId, FlowStateKey},
        trace::QualifiedEvent,
    },
    api::{
        classification::ClassificationEvidenceOccurrence,
        compiler::{CompiledObjectRequirement, CompiledObjectSinkArguments},
    },
    project::EvidenceRole,
};

impl ObjectFlowProjector<'_, '_, '_> {
    /// Apply member-call requirements to live object states.
    pub(super) fn record_configuration(
        &mut self,
        receiver: Option<ValueId>,
        chain: &NamePath,
        args: &[CallArgInfo],
        event: FactId,
    ) {
        let objects: Vec<ObjectId> = match receiver {
            Some(value) => self.object_for(value).into_iter().collect(),
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
                                matcher
                                    .predicate()
                                    .matches(arg, self.names, self.stream.values())
                            })
                        })
                    {
                        state.record_requirement(index, event);
                    }
                }
                drop(state);
                self.emit_if_ready(key.flow, key.object, event);
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
        let flow_ids: Vec<FlowId> = flow_ids.to_vec();
        for (argument_index, argument) in args.iter().enumerate() {
            let Some(object) = self.object_for(argument.value) else {
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
                    let state = self.flow_state.state(key.object, key.flow).cloned();
                    let Some(state) = state else {
                        continue;
                    };
                    let ready = self
                        .plan
                        .get(flow_id)
                        .is_some_and(|flow| state.is_ready(flow));
                    if !ready {
                        continue;
                    }
                    self.emit_state(&state, sink_fact);
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
        let Some(summary_ref) = self.helpers.get(function) else {
            return;
        };
        if !summary_ref.is_invocation_compatible(self.stream, args, self.helpers.path_interner()) {
            return;
        }
        let summary = summary_ref.clone();
        let parameters = summary.parameter_bindings(self.stream).to_vec();
        let values: Vec<(FlowId, ValueId)> = summary
            .sinks()
            .into_iter()
            .filter_map(|sink| {
                let value = {
                    let paths = self.helpers.path_interner();
                    let parameter = parameters.iter().find(|parameter| {
                        parameter.parameter_index == sink.parameter_index()
                            && (SummaryPathStore::matches_frozen(sink.path(), parameter.path)
                                || (parameter.rest
                                    && paths.starts_with_frozen(sink.path(), parameter.path)))
                    })?;
                    parameter.project_argument_at(self.stream, args, paths, sink.path())?
                };
                Some((sink.flow(), value))
            })
            .collect();
        let ready: Vec<(ObjectId, FlowId)> = values
            .into_iter()
            .filter_map(|(flow_id, value)| {
                let object = self.object_for(value)?;
                let state = self.flow_state.state(object, flow_id)?;
                let flow = self.plan.get(flow_id)?;
                state.is_ready(flow).then_some((object, flow_id))
            })
            .collect();
        for (object, flow_id) in ready {
            let state = self.flow_state.state(object, flow_id).cloned();
            let Some(state) = state else {
                continue;
            };
            let ready = self
                .plan
                .get(flow_id)
                .is_some_and(|flow| state.is_ready(flow));
            if !ready {
                continue;
            }
            self.emit_state(&state, sink_fact);
        }
    }

    /// Emit a requirement-only match when its state is complete.
    pub(super) fn emit_if_ready(&mut self, flow: FlowId, object: ObjectId, event: FactId) {
        let state = self.flow_state.state(object, flow).cloned();
        let Some(state) = state else {
            return;
        };
        let ready = self
            .plan
            .get(flow)
            .is_some_and(|f| f.emit_on_requirements && state.is_ready(f));
        if !ready {
            return;
        }
        self.emit_state(&state, event);
    }

    /// Defer one ready state until every alternative reaching the fact has
    /// been evaluated. This keeps certainty about path coverage separate from
    /// the witness trace itself.
    fn emit_state(&mut self, state: &FlowState, match_fact: FactId) {
        if self.emission_mode == super::EmissionMode::Replay {
            return;
        }
        self.queue_state(state.clone(), match_fact);
    }

    pub(super) fn emit_state_final(
        &mut self,
        state: &FlowState,
        match_fact: FactId,
        certainty: crate::project::MatchCertainty,
    ) {
        debug_assert!(state.source_event() <= match_fact);
        let key = ReportEvidenceKey::new(
            state.flow_id().rule_index().get(),
            state.flow_id().flow_index(),
            state.object_id(),
            match_fact,
        );

        // Extract the flow symbol before the mutable borrow of self.
        let flow_symbol = self
            .plan
            .get(state.flow_id())
            .map(crate::api::compiler::object_flow::CompiledObjectFlow::evidence_symbol)
            .unwrap_or_default();

        if !self
            .flow_evidence
            .try_insert(key, self.limits.emission_limit(), 256)
        {
            return;
        }

        let anchor = match_fact;
        let span = self
            .stream
            .fact(anchor)
            .map_or(glass_lint_datastructures::ByteRange::empty(), |fact| {
                fact.span
            });

        // Build the trace chain: Source → Requirements (in declaration order) → Sink.
        let Some(trace_head) = self.build_flow_trace(state, match_fact) else {
            return;
        };

        self.flow_evidence.record(
            state.flow_id().rule_index().get(),
            ClassificationEvidence {
                kind: MatchKind::CallArgument,
                symbol: flow_symbol,
                count: 1,
                truncated: false,
                certainty,
                occurrences: vec![ClassificationEvidenceOccurrence {
                    span,
                    fact: Some(anchor.0),
                    trace: Some(trace_head),
                }],
            },
        );
        self.trace_heads = self.trace_heads.saturating_add(1);
    }

    /// Build an interned trace chain for a flow finding:
    /// Source → Requirement[0] → Requirement[1] → ... → Sink.
    /// Returns `None` if the trace arena is exhausted.
    fn build_flow_trace(
        &mut self,
        state: &FlowState,
        sink_fact: FactId,
    ) -> Option<crate::api::classification::TraceNodeId> {
        // 1. Source node (first in execution order, no parent)
        let source_node = self.trace_arena.intern(
            None,
            QualifiedEvent::new(self.module_id, state.source_event()),
            EvidenceRole::Source,
        )?;

        // 2. Requirement nodes (declaration order by index)
        let mut tail: Option<crate::api::classification::TraceNodeId> = Some(source_node);
        for (_index, values) in state.requirement_keys() {
            // Use the first (deterministic) value per requirement key for the trace.
            let first_val = match values.first() {
                Some(v) => *v,
                None => continue,
            };
            let next = self.trace_arena.intern(
                tail,
                QualifiedEvent::new(self.module_id, first_val),
                EvidenceRole::Requirement,
            );
            #[allow(clippy::question_mark)]
            if next.is_none() {
                return None;
            }
            tail = next;
        }

        // 3. Sink node (last in execution order, becomes the trace head)
        self.trace_arena.intern(
            tail,
            QualifiedEvent::new(self.module_id, sink_fact),
            EvidenceRole::Sink,
        )
    }
}
