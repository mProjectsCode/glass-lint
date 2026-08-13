//! Evidence emission and flow requirement updates.
//!
//! Configuration events update only the object states reachable through proven
//! aliases. Emissions are anchored at the event that completed the flow and
//! deduplicated by flow/object/event before the bounded result is returned.

use glass_lint_datastructures::NamePath;
use smallvec::SmallVec;

use crate::{
    analysis::{
        flow::{
            effect::CallShape,
            planning::FlowMatchView,
            projector::{
                CallArgInfo, ClassificationEvidence, FactId, FlowState, MatchKind,
                ObjectFlowProjector, ObjectId, ValueId, history::ReportEvidenceKey,
            },
        },
        model::{
            flow::{FlowId, FlowStateKey},
            scope::FunctionId,
        },
        trace::{QualifiedEvent, TraceNodeId, intern_lifecycle_trace},
    },
    api::{
        classification::ClassificationEvidenceOccurrence, compiler::object_flow::CompletionMode,
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
        let matcher = FlowMatchView::new(self.inputs.names, self.inputs.stream.values());
        let objects: SmallVec<[ObjectId; 4]> = match receiver {
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
                for index in self.inputs.plan.matching_member_requirement_indices(
                    key.flow(),
                    Some(chain),
                    args,
                    &matcher,
                ) {
                    self.flow_state
                        .record_requirement(key.object(), key.flow(), index, event);
                }
                self.emit_if_ready(key.flow(), key.object(), event);
            }
        }
    }

    /// Check sink arguments against live states and emit completed flows.
    pub(super) fn record_sinks(
        &mut self,
        call: &CallShape<'_>,
        args: &[CallArgInfo],
        sink_fact: FactId,
    ) {
        let candidates = call
            .global_name()
            .and_then(|name| self.inputs.plan.global_sink_candidates(name))
            .or_else(|| {
                call.rooted().then_some(()).and_then(|()| {
                    call.chain()
                        .and_then(|chain| self.inputs.plan.sink_candidates(chain))
                })
            })
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        if candidates.is_empty() {
            return;
        }
        for (argument_index, argument) in args.iter().enumerate() {
            let Some(object) = self.object_for(argument.value) else {
                continue;
            };
            let pairs: SmallVec<[(FlowStateKey, FlowId); 8]> = self
                .flow_state
                .states_for(object)
                .filter(|(key, _)| {
                    candidates.iter().any(|sink| {
                        sink.flow_id() == key.flow() && sink.matches_argument(argument_index)
                    })
                })
                .map(|(key, _)| (key, key.flow()))
                .collect();
            for (key, flow_id) in pairs {
                let matching_sinks: SmallVec<[_; 4]> = candidates
                    .iter()
                    .filter(|sink| {
                        sink.flow_id() == flow_id && sink.matches_argument(argument_index)
                    })
                    .map(crate::analysis::flow::planning::BoundSink::index)
                    .collect();
                if !matching_sinks.is_empty() {
                    for index in matching_sinks {
                        self.flow_state
                            .record_sink(key.object(), key.flow(), index, sink_fact);
                    }
                    self.emit_completed_sink(key.object(), flow_id, sink_fact);
                }
            }
        }
    }

    /// Project a summarized helper sink through a concrete invocation.
    pub(super) fn record_helper_sink(
        &mut self,
        function: FunctionId,
        args: &[CallArgInfo],
        sink_fact: FactId,
    ) {
        let Some(summary_ref) = self.inputs.helpers.get(function) else {
            return;
        };
        if !summary_ref.is_invocation_compatible(
            self.inputs.stream,
            args,
            self.inputs.helpers.path_interner(),
        ) {
            return;
        }
        let summary = summary_ref.clone();
        let Some(parameters) = summary.parameter_bindings(self.inputs.stream) else {
            return;
        };
        let parameters = parameters.to_vec();
        #[allow(clippy::needless_collect)]
        let values: Vec<(FlowId, ValueId)> = summary
            .sinks()
            .into_iter()
            .filter_map(|sink| {
                let value = {
                    let paths = self.inputs.helpers.path_interner();
                    let parameter = parameters.iter().find(|parameter| {
                        parameter.parameter_index() == sink.parameter_index()
                            && parameter.matches_sink_path(sink.path(), paths)
                    })?;
                    parameter.project_argument_at(self.inputs.stream, args, paths, sink.path())?
                };
                Some((sink.flow(), value))
            })
            .collect();
        let ready: Vec<(ObjectId, FlowId)> = values
            .into_iter()
            .filter_map(|(flow_id, value)| {
                let object = self.object_for(value)?;
                let state = self.flow_state.state(object, flow_id)?;
                let flow = self.inputs.plan.get(flow_id)?;
                state
                    .is_ready(flow.readiness())
                    .then_some((object, flow_id))
            })
            .collect();
        for (object, flow_id) in ready {
            self.emit_completed_sink(object, flow_id, sink_fact);
        }
    }

    fn emit_completed_sink(&mut self, object: ObjectId, flow: FlowId, sink_fact: FactId) {
        let state = self.flow_state.state(object, flow).cloned();
        let Some(state) = state else {
            return;
        };
        let ready = self.inputs.plan.get(flow).is_some_and(|flow| {
            let readiness = flow.readiness();
            state.is_ready(readiness) && state.sinks_ready(readiness)
        });
        if ready {
            self.emit_state(&state, sink_fact);
        }
    }

    /// Emit a requirement-only match when its state is complete.
    pub(super) fn emit_if_ready(&mut self, flow: FlowId, object: ObjectId, event: FactId) {
        let state = self.flow_state.state(object, flow).cloned();
        let Some(state) = state else {
            return;
        };
        let ready = self.inputs.plan.get(flow).is_some_and(|f| {
            f.completion_mode() == CompletionMode::Configuration && state.is_ready(f.readiness())
        });
        if !ready {
            return;
        }
        self.emit_state(&state, event);
    }

    /// Defer one ready state until every alternative reaching the fact has
    /// been evaluated. This keeps certainty about path coverage separate from
    /// the witness trace itself.
    fn emit_state(&mut self, state: &FlowState, match_fact: FactId) {
        if self.run.emission_mode == super::EmissionMode::Replay {
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
            state.flow_id().rule_index(),
            state.flow_id().flow_index(),
            state.object_id(),
            match_fact,
        );

        // Extract the flow symbol before the mutable borrow of self.
        let flow_symbol: String = self
            .inputs
            .plan
            .get(state.flow_id())
            .map(|f| f.evidence_symbol().as_str())
            .unwrap_or_default()
            .to_owned();

        let anchor = match_fact;
        let span = self
            .inputs
            .stream
            .fact(anchor)
            .map_or(glass_lint_datastructures::ByteRange::empty(), |fact| {
                fact.span
            });

        // Build the trace chain: Source → Requirements (in declaration order) → Sink.
        let Some(trace_head) = self.build_flow_trace(state, match_fact) else {
            return;
        };

        let evidence = ClassificationEvidence::from_occurrence(
            MatchKind::CallArgument,
            flow_symbol,
            ClassificationEvidenceOccurrence::new(span, Some(anchor.raw()), Some(trace_head)),
            certainty,
        );
        if !self.flow_evidence.record_if_admitted(
            key,
            self.run.limits.emission_limit(),
            256,
            state.flow_id().rule_index(),
            evidence,
        ) {
            return;
        }
        self.run.trace_heads = self.run.trace_heads.saturating_add(1);
    }

    /// Build an interned trace chain for a flow finding:
    /// Source → Requirement[0] → Requirement[1] → ... → Sink.
    /// Returns `None` if the trace arena is exhausted.
    fn build_flow_trace(&mut self, state: &FlowState, sink_fact: FactId) -> Option<TraceNodeId> {
        let requirements = state
            .requirement_entries()
            .filter_map(|(_index, values)| values.into_iter().next())
            .map(|fact| QualifiedEvent::new(self.inputs.module_id, fact));
        let prior_sinks = state
            .prior_sinks(sink_fact)
            .into_iter()
            .map(|fact| QualifiedEvent::new(self.inputs.module_id, fact));
        intern_lifecycle_trace(
            self.trace_arena,
            QualifiedEvent::new(self.inputs.module_id, state.source_event()),
            requirements,
            prior_sinks,
            QualifiedEvent::new(self.inputs.module_id, sink_fact),
            EvidenceRole::Requirement,
        )
    }
}
