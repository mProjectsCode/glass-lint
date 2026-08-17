use std::collections::BTreeSet;

use super::{
    AbruptExit, ControlFrame, FlowEnvironment, FlowEvidence, FlowSemanticSnapshot, FlowStateTable,
    LocalFlowProjectionOutcome, ObjectFlowProjector, PendingFlowKey, PendingState,
    ProjectionInputs, ProjectionPathMachine, ProjectionRunState, PropertyWriteUpdate,
    loops::LoopFixedPoint, state::LoopSeed,
};
use crate::{
    analysis::{
        facts::{FactId, FactPayload, FunctionBoundary},
        flow::FlowCompletion,
        model::{
            flow::{FlowLimits, FlowState},
            value::{FlowObjectId, ValueId},
        },
        trace::TraceArena,
    },
    api::{classification::RuleEvidenceTable, compiler::object_flow::CompletionMode},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis::flow::projector) enum EmissionMode {
    Emit,
    Replay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis::flow::projector) enum PathAdmission {
    Admitted,
    Duplicate,
    RestoreFailed,
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathRestoration {
    Ready,
    Failed,
    Exhausted,
}

impl<'rules, 'stream, 'arena> ObjectFlowProjector<'rules, 'stream, 'arena> {
    pub(super) fn new(
        inputs: ProjectionInputs<'rules, 'stream>,
        evidence: &'stream mut RuleEvidenceTable,
        limits: FlowLimits,
        completion: FlowCompletion,
        trace_arena: &'arena mut TraceArena,
    ) -> Self {
        Self {
            inputs,
            flow_evidence: FlowEvidence::new(evidence, limits.emission_limit()),
            flow_state: FlowStateTable::new(limits.state_limit(), limits.mutation_limit()),
            run: ProjectionRunState::new(limits, completion),
            paths: ProjectionPathMachine::initial(),
            trace_arena,
        }
    }

    fn observe_alternatives(&mut self, count: usize) {
        self.run.max_live_alternatives = self.run.max_live_alternatives.max(count);
    }

    pub(super) fn mark_incomplete(&mut self) {
        self.run.mark_incomplete();
    }

    pub(super) fn transfer(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        match fact.payload() {
            FactPayload::Function { boundary, .. } => self.transfer_function(*boundary),
            FactPayload::Control { kind, region } => {
                self.transfer_control(*kind, *region, fact.id());
            }
            FactPayload::Return { .. } => self.transfer_abrupt(AbruptExit::Return),
            FactPayload::Break => self.transfer_abrupt(AbruptExit::Break),
            FactPayload::Continue => self.transfer_abrupt(AbruptExit::Continue),
            _ => self.transfer_paths(fact),
        }
    }

    fn transfer_paths(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        self.transfer_paths_with(|projector| projector.transfer_fact(fact), true);
    }

    fn transfer_paths_with(&mut self, transfer: impl Fn(&mut Self), finalize: bool) {
        let incoming = self.paths.frontier.take_paths();
        if finalize && incoming.is_empty() {
            return;
        }
        self.paths.frontier.begin_batch(incoming.len());
        let mut outgoing = Vec::with_capacity(incoming.len());
        for (path_index, environment) in incoming.into_iter().enumerate() {
            match self.restore_path(environment) {
                PathRestoration::Exhausted => break,
                PathRestoration::Failed => continue,
                PathRestoration::Ready => {}
            }
            self.paths.frontier.select_path(path_index);
            transfer(self);
            if self.run.reachable {
                outgoing.push(self.environment());
            }
        }
        self.paths.frontier.replace_paths(outgoing);
        self.observe_alternatives(self.paths.frontier.path_count());
        if finalize {
            self.finalize_pending();
        }
        let paths = self.paths.frontier.take_paths();
        self.join_paths(paths);
        self.paths.frontier.end_batch();
    }

    fn transfer_fact(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        match fact.payload() {
            FactPayload::Declaration { target, source } => self.assign(*target, *source),
            FactPayload::Assignment {
                target,
                source,
                receiver,
            } => {
                if let Some(receiver) = receiver {
                    self.invalidate_object(*receiver);
                } else {
                    self.assign(*target, *source);
                }
            }
            FactPayload::PropertyWrite {
                receiver,
                property,
                value,
                rooted_chain: _,
                value_is_precise,
            } => {
                let static_string = self.inputs.stream.values().static_string(*value);
                self.record_property_write(
                    *receiver,
                    property.and_then(|id| self.inputs.stream.resolve_name(id)),
                    static_string,
                    *value_is_precise,
                    fact.id(),
                );
            }
            FactPayload::Call(_) => self.transfer_call(fact),
            _ => {}
        }
    }

    /// Replay one loop body from a set of back-edge environments.  The body
    /// is already represented by the canonical fact stream, so replaying that
    /// bounded slice does not add an AST traversal or a second semantic model.
    pub(super) fn replay_loop_body(
        &mut self,
        body_start: FactId,
        body_end: FactId,
        input: Vec<FlowEnvironment>,
    ) -> Vec<FlowEnvironment> {
        let (Some(start), Some(end)) = (body_start.index(), body_end.index()) else {
            self.run.mark_incomplete();
            return Vec::new();
        };
        if start >= end || end > self.inputs.stream.facts().len() {
            self.run.mark_incomplete();
            return Vec::new();
        }
        let stream = self.inputs.stream;
        let previous_mode = self.run.emission_mode;
        self.run.emission_mode = EmissionMode::Replay;
        self.paths.frontier.replace_paths(input);
        for i in start..end {
            let fact = &stream.facts()[i];
            self.transfer(fact);
        }
        self.run.emission_mode = previous_mode;
        self.paths.frontier.take_paths()
    }

    /// Compute the bounded loop back-edge closure.  A semantic state is
    /// canonicalized before it is admitted to the frontier, which prevents
    /// fresh object allocation in repeated iterations from becoming an
    /// unbounded sequence of equivalent alternatives.
    pub(super) fn finish_loop(&mut self, mut seed: LoopSeed, body_end: FactId) {
        let mut entrance = self.paths.frontier.take_paths();
        entrance.append(&mut seed.continues);
        self.join_paths(entrance);
        let entrance = self.paths.frontier.take_paths();

        let mut fixed_point = LoopFixedPoint::start(
            entrance,
            seed.baseline,
            seed.guaranteed,
            seed.breaks,
            self.run.limits.alternative_limit(),
        );
        fixed_point.converge(&mut *self, seed.body_start, body_end);

        if self.paths.control.pop_loop().is_err() {
            self.mark_incomplete();
            return;
        }

        let outcome = fixed_point.collect_exits(self);
        if !outcome.complete {
            self.run.mark_incomplete();
        }
        self.join_paths(outcome.exits);
    }

    fn transfer_function(&mut self, boundary: FunctionBoundary) {
        match boundary {
            FunctionBoundary::Enter => {
                let caller = self.paths.frontier.snapshot_paths();
                self.paths.control.push(ControlFrame::Function { caller });
                self.transfer_paths_with(
                    |projector| {
                        projector.flow_state.clear();
                        projector.run.reachable = true;
                    },
                    false,
                );
            }
            FunctionBoundary::Exit => match self.paths.control.pop_function() {
                Ok(caller) => self.paths.frontier.replace_paths(caller),
                Err(_) => self.mark_incomplete(),
            },
        }
    }

    fn restore_path(&mut self, environment: FlowEnvironment) -> PathRestoration {
        if !self.run.charge_operation() {
            return PathRestoration::Exhausted;
        }
        if !self.flow_state.restore(environment) {
            self.run.mark_incomplete();
            return PathRestoration::Failed;
        }
        self.run.reachable = environment.is_reachable();
        PathRestoration::Ready
    }

    fn transfer_call(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        if !self.run.reachable {
            return;
        }
        let FactPayload::Call(call) = fact.payload() else {
            return;
        };
        let cref = self.inputs.stream.call_effect(fact.id());
        let Some(shape) = cref.shape() else {
            if let Some(function) = call.target_function() {
                self.record_helper_sink(function, call.args(), fact.id());
            }
            return;
        };
        let effective_args = shape.effective_args();
        if let Some(chain) = shape.chain() {
            self.record_configuration(call.receiver(), chain, effective_args, fact.id());
        }
        self.record_sinks(&shape, effective_args, fact.id());
        if let Some(function) = call.target_function() {
            self.record_helper_sink(function, call.args(), fact.id());
        }
    }

    fn environment(&self) -> FlowEnvironment {
        self.flow_state.capture(self.run.reachable)
    }

    pub(super) fn object_for(&mut self, value: ValueId) -> Option<FlowObjectId> {
        let aliases = self.value_aliases(value);
        self.flow_state.object_for_any(&aliases)
    }

    /// Charge, restore, canonicalize, and deduplicate one environment.
    ///
    /// Restoration failure marks the run incomplete, and neither failure mode
    /// can establish a complete witness. The caller retains ownership of the
    /// admitted environment so joins and loop frontiers can preserve their
    /// distinct collection policies.
    pub(super) fn admit_path(
        &mut self,
        seen: &mut BTreeSet<FlowSemanticSnapshot>,
        environment: FlowEnvironment,
    ) -> PathAdmission {
        if !self.run.charge_operation() {
            return PathAdmission::Exhausted;
        }
        if !self.flow_state.restore(environment) {
            self.run.mark_incomplete();
            return PathAdmission::RestoreFailed;
        }
        if seen.insert(self.flow_state.semantic_snapshot()) {
            PathAdmission::Admitted
        } else {
            PathAdmission::Duplicate
        }
    }

    pub(super) fn join_paths(&mut self, mut paths: Vec<FlowEnvironment>) {
        self.observe_alternatives(paths.len());
        paths.retain(FlowEnvironment::is_reachable);
        let mut unique = Vec::with_capacity(paths.len());
        let mut seen_snapshots = BTreeSet::new();
        for path in paths {
            match self.admit_path(&mut seen_snapshots, path) {
                PathAdmission::Exhausted => break,
                PathAdmission::RestoreFailed | PathAdmission::Duplicate => {}
                PathAdmission::Admitted => {
                    if seen_snapshots.len() > 1 {
                        self.run.coalescing_comparisons =
                            self.run.coalescing_comparisons.saturating_add(1);
                    }
                    unique.push(path);
                }
            }
        }
        paths = unique;
        if paths.len() > self.run.limits.alternative_limit() {
            paths.truncate(self.run.limits.alternative_limit());
            self.run.mark_incomplete();
        }
        self.paths.frontier.replace_paths(paths);
        self.observe_alternatives(self.paths.frontier.path_count());
        self.run.reachable = self.paths.frontier.has_paths();
    }

    fn finalize_pending(&mut self) {
        let Some(active_paths) = self.paths.frontier.active_paths() else {
            return;
        };
        let finalized = self
            .paths
            .pending
            .finalize(active_paths, self.run.alternatives_complete);
        for (event, certainty, states) in finalized {
            for pending in states {
                self.emit_state_final(&pending.state, event, certainty);
            }
        }
    }

    pub(super) fn queue_state(&mut self, state: FlowState, event: FactId) {
        let Some(path) = self.paths.frontier.active_path() else {
            return;
        };
        self.paths
            .pending
            .entry(PendingFlowKey {
                flow: state.flow_id(),
                event,
            })
            .push(PendingState { path, state });
    }

    fn record_property_write(
        &mut self,
        receiver: ValueId,
        property: Option<&str>,
        value: Option<&str>,
        value_is_precise: bool,
        event: FactId,
    ) {
        let Some(object) = self.object_for(receiver) else {
            return;
        };
        let updated = self
            .flow_state
            .apply_property_write(object, event, |flow_id| {
                self.inputs
                    .plan
                    .matching_property_requirements(flow_id, property, value, value_is_precise)
                    .into_iter()
                    .map(|match_result| PropertyWriteUpdate {
                        index: match_result.index(),
                        value_matches: match_result.value_matches(),
                    })
                    .collect()
            });
        for flow in updated {
            self.emit_if(
                object,
                flow,
                event,
                Some(CompletionMode::Configuration),
                false,
            );
        }
    }

    pub(in crate::analysis::flow::projector) fn unbind_value(&mut self, value: ValueId) {
        let aliases = self.value_aliases(value);
        self.flow_state.unbind_aliases(&aliases);
    }

    pub(in crate::analysis::flow::projector) fn bind_value(
        &mut self,
        value: ValueId,
        object: FlowObjectId,
    ) {
        let aliases = self.value_aliases(value);
        self.flow_state.bind_aliases(&aliases, object);
    }

    pub(in crate::analysis::flow::projector) fn value_aliases(
        &mut self,
        value: ValueId,
    ) -> Vec<ValueId> {
        let mut values = vec![value];
        if let Some(resolved) = self.inputs.stream.values().resolve_id(value)
            && resolved != value
        {
            values.push(resolved);
        }
        if let Some(slot) = self.inputs.stream.values().binding_slot(value) {
            let representative = *self.paths.binding_slots.entry(slot).or_insert(value);
            if !values.contains(&representative) {
                values.push(representative);
            }
        }
        values
    }

    fn invalidate_object(&mut self, value: ValueId) {
        let aliases = self.value_aliases(value);
        self.flow_state.invalidate_aliases(&aliases);
    }

    pub(in crate::analysis::flow::projector) fn allocate_object_id(
        &mut self,
    ) -> Option<FlowObjectId> {
        if self.run.next_object_id >= self.run.limits.object_limit() {
            self.run.object_limit_rejected = true;
            return None;
        }
        let object = FlowObjectId::new(self.run.next_object_id);
        self.run.next_object_id = self.run.next_object_id.checked_add(1)?;
        Some(object)
    }

    /// Consume the projector and produce a bounded summary of what was used.
    pub(super) fn into_outcome(self) -> LocalFlowProjectionOutcome {
        let mut flow_evidence = self.flow_evidence;
        flow_evidence.mark_truncated();
        let completion = FlowCompletion::from_sources(
            &self.run,
            &self.flow_state,
            &flow_evidence,
            self.trace_arena,
        );
        if completion.is_incomplete() {
            flow_evidence.mark_all_possible();
        }
        LocalFlowProjectionOutcome {
            completion,
            operations: self.run.operation_budget.used(),
            max_live_alternatives: self.run.max_live_alternatives,
            coalescing_comparisons: self.run.coalescing_comparisons,
            fixed_point_iterations: self.run.fixed_point_iterations,
            trace_heads: self.run.trace_heads,
        }
    }
}
