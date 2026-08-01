//! Bounded object flow over the canonical semantic fact stream.
//!
//! This projector owns no AST and performs no resolution.  `FactBuilder` has
//! already assigned value identities, effective call arguments, member chains,
//! and function targets.  The transfer state below only follows those typed
//! identities and emits evidence at canonical call sites.
//!
//! Control frames snapshot environments at joins, while transfer modules
//! update aliases and lifecycle requirements. Any unsupported or over-budget
//! path is discarded rather than converted into a speculative finding.

mod control;
mod evidence;
mod history;
mod state;
mod transfer;

use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::{Budget, NameTable};
use state::{AbruptExit, ControlFrame, FlowEnvironment, FlowEvidence, FlowStateTable};

use crate::{
    analysis::{
        facts::{
            CallArgInfo, ControlKind, FactId, FactPayload, FactStream, Frozen, FunctionBoundary,
        },
        flow::{
            effect::{CallEffectRef, FunctionEffects},
            planning::BoundFlowPlan,
            summary::FunctionSummaries,
        },
        model::flow::{FlowId, FlowLimits, FlowState},
        trace::TraceArena,
        value::{ObjectId, ValueId},
    },
    api::{
        classification::{ClassificationEvidence, MatchKind, RuleEvidenceTable, RuleIndex},
        compiler::{CompiledObjectFlow, CompiledObjectRequirement},
    },
    project::ModuleId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmissionMode {
    Emit,
    Replay,
}

/// Exhaustion state and bounded counters returned by local flow projection.
#[derive(Debug, Clone, Copy, Default)]
pub(in crate::analysis) struct LocalFlowProjectionOutcome {
    /// Whether any budget was exhausted during projection.
    pub exhausted: bool,
    /// Object identities allocated.
    #[allow(dead_code)]
    pub objects_used: u32,
    /// Charged local flow operations.
    pub operations: usize,
    /// Maximum number of correlated alternatives retained at one point.
    pub max_live_alternatives: usize,
    /// Number of semantic-state comparisons made while coalescing paths.
    pub coalescing_comparisons: usize,
    /// Number of loop fixed-point iterations.
    pub fixed_point_iterations: usize,
    /// Number of complete trace heads emitted by local flow.
    pub trace_heads: usize,
}

/// Push flow evidence directly into an externally-owned per-rule vec,
/// avoiding a separate evidence matrix allocation alongside the caller's.
/// Returns the exhaustion state and bounded counters for the caller.
pub(in crate::analysis) fn collect_into(
    stream: &FactStream<Frozen>,
    effects: &FunctionEffects,
    rules: &[(RuleIndex, usize, &CompiledObjectFlow)],
    evidence: &mut RuleEvidenceTable,
    limits: FlowLimits,
    module_id: ModuleId,
    trace_arena: &mut TraceArena,
) -> LocalFlowProjectionOutcome {
    if rules.is_empty() {
        return LocalFlowProjectionOutcome::default();
    }
    let names = stream.names();
    let plan = BoundFlowPlan::new(rules, names);
    let mut summary_budget = Budget::new(limits.emission_limit());
    let helpers = FunctionSummaries::collect(stream, effects, &plan, &mut summary_budget);
    let mut projector = ObjectFlowProjector::new(ObjectFlowProjectorInput {
        stream,
        names,
        plan,
        helpers,
        evidence: evidence.as_mut_slices(),
        limits,
        summary_exhausted: summary_budget.exhausted(),
        module_id,
        trace_arena,
    });
    for fact in stream.facts() {
        projector.transfer(fact);
    }
    projector.into_outcome()
}

#[cfg(test)]
pub(super) fn collect_with_limits(
    stream: &FactStream<Frozen>,
    effects: &FunctionEffects,
    rules: &[(RuleIndex, usize, &CompiledObjectFlow)],
    rule_count: usize,
    limits: FlowLimits,
    module_id: ModuleId,
    trace_arena: &mut TraceArena,
) -> (RuleEvidenceTable, LocalFlowProjectionOutcome) {
    let mut evidence = RuleEvidenceTable::new(rule_count);
    let outcome = collect_into(
        stream,
        effects,
        rules,
        &mut evidence,
        limits,
        module_id,
        trace_arena,
    );
    (evidence, outcome)
}

#[derive(Debug)]
struct ObjectFlowProjector<'rules, 'stream, 'arena> {
    /// The canonical facts are the projector's only input. In particular, it
    /// must never inspect the AST or reconstruct resolution decisions.
    stream: &'stream FactStream<Frozen>,
    names: &'stream NameTable,
    plan: BoundFlowPlan<'rules>,
    helpers: FunctionSummaries<'stream>,
    /// Call results are indexed once so later assignments can start a flow
    /// without rescanning the fact stream.
    calls_by_result: BTreeMap<ValueId, FactId>,
    /// Evidence is grouped and deduplicated by the flow-specific evidence
    /// owner.
    flow_evidence: FlowEvidence<'stream>,
    /// Each value identity and live object-flow state are owned together.
    flow_state: FlowStateTable,
    /// Bounded lifecycle, allocation, emission, and outcome state for one run.
    run: ProjectionRunState,
    /// Nested branch/function frames used to restore environments at joins.
    control: Vec<ControlFrame>,
    /// Correlated checkpoint-backed alternatives reaching the next fact.
    paths: Vec<FlowEnvironment>,
    /// Fact-local witnesses are finalized after every reaching alternative has
    /// seen the sink or requirement event.
    pending: BTreeMap<(usize, FlowId, FactId), Vec<(usize, FlowState)>>,
    /// Number of alternatives in the current fact-local replay batch.
    active_path_count: usize,
    active_path_index: usize,
    /// Stable representative value for each lexical binding slot. Binding
    /// versions differ at joins, but the slot remains the same variable.
    binding_slots: BTreeMap<
        (
            crate::analysis::value::FunctionId,
            crate::analysis::value::BindingId,
            glass_lint_datastructures::NamePath,
        ),
        ValueId,
    >,
    /// Shared trace arena for interning evidence trace nodes.
    trace_arena: &'arena mut TraceArena,
    /// Module being projected, used to qualify trace events.
    module_id: ModuleId,
}

#[derive(Debug)]
struct ProjectionRunState {
    limits: FlowLimits,
    next_object_id: u32,
    object_limit_rejected: bool,
    alternatives_complete: bool,
    reachable: bool,
    summary_exhausted: bool,
    /// Suppress findings while replaying a loop body to compute its fixed
    /// point; replay only propagates semantic state.
    emission_mode: EmissionMode,
    operation_budget: Budget,
    max_live_alternatives: usize,
    coalescing_comparisons: usize,
    fixed_point_iterations: usize,
    trace_heads: usize,
}

impl ProjectionRunState {
    fn new(limits: FlowLimits, summary_exhausted: bool) -> Self {
        Self {
            limits,
            next_object_id: 0,
            object_limit_rejected: false,
            alternatives_complete: true,
            reachable: true,
            summary_exhausted,
            emission_mode: EmissionMode::Emit,
            operation_budget: Budget::new(limits.local_operation_limit()),
            max_live_alternatives: 1,
            coalescing_comparisons: 0,
            fixed_point_iterations: 0,
            trace_heads: 0,
        }
    }
}

struct ObjectFlowProjectorInput<'rules, 'stream, 'arena> {
    stream: &'stream FactStream<Frozen>,
    names: &'stream NameTable,
    plan: BoundFlowPlan<'rules>,
    helpers: FunctionSummaries<'stream>,
    evidence: &'stream mut [Vec<ClassificationEvidence>],
    limits: FlowLimits,
    summary_exhausted: bool,
    module_id: ModuleId,
    trace_arena: &'arena mut TraceArena,
}

impl<'rules, 'stream, 'arena> ObjectFlowProjector<'rules, 'stream, 'arena> {
    fn new(input: ObjectFlowProjectorInput<'rules, 'stream, 'arena>) -> Self {
        let ObjectFlowProjectorInput {
            stream,
            names,
            plan,
            helpers,
            evidence,
            limits,
            summary_exhausted,
            module_id,
            trace_arena,
        } = input;
        let calls_by_result = stream
            .facts()
            .iter()
            .filter_map(|fact| match &fact.payload {
                FactPayload::Call { result, .. } => Some((*result, fact.id)),
                _ => None,
            })
            .collect();
        Self {
            stream,
            names,
            plan,
            helpers,
            calls_by_result,
            flow_evidence: FlowEvidence::new(evidence),
            flow_state: FlowStateTable::new(limits.state_limit(), limits.mutation_limit()),
            run: ProjectionRunState::new(limits, summary_exhausted),
            control: Vec::new(),
            paths: vec![FlowEnvironment::initial()],
            pending: BTreeMap::new(),
            active_path_count: 0,
            active_path_index: 0,
            binding_slots: BTreeMap::new(),
            module_id,
            trace_arena,
        }
    }

    fn charge_operation(&mut self) -> bool {
        if self.run.operation_budget.try_push() {
            true
        } else {
            self.run.alternatives_complete = false;
            false
        }
    }

    fn observe_alternatives(&mut self, count: usize) {
        self.run.max_live_alternatives = self.run.max_live_alternatives.max(count);
    }

    fn transfer(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        match &fact.payload {
            FactPayload::Function { boundary, .. } => self.transfer_function(*boundary),
            FactPayload::Control { kind, region, .. } => {
                self.transfer_control(*kind, *region, fact.id);
            }
            _ => self.transfer_paths(fact),
        }
    }

    fn transfer_paths(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        let incoming = std::mem::take(&mut self.paths);
        if incoming.is_empty() {
            return;
        }
        self.active_path_count = incoming.len();
        let mut outgoing = Vec::with_capacity(incoming.len());
        for (path_index, environment) in incoming.into_iter().enumerate() {
            if !self.charge_operation() {
                break;
            }
            if !self.flow_state.restore(environment) {
                self.run.alternatives_complete = false;
                continue;
            }
            self.run.reachable = environment.reachable();
            self.active_path_index = path_index;
            self.transfer_fact(fact);
            if self.run.reachable {
                outgoing.push(self.environment());
            }
        }
        self.paths = outgoing;
        self.observe_alternatives(self.paths.len());
        self.finalize_pending(fact.id);
        let paths = std::mem::take(&mut self.paths);
        self.join_paths(paths);
        self.active_path_count = 0;
    }

    fn transfer_fact(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        match &fact.payload {
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
                let static_string = self.stream.values().static_string(*value);
                self.record_property_write(
                    *receiver,
                    property.and_then(|id| self.stream.resolve_name(id)),
                    static_string,
                    *value_is_precise,
                    fact.id,
                );
            }
            FactPayload::Call { .. } => self.transfer_call(fact),
            _ => {}
        }
    }

    /// Replay one loop body from a set of back-edge environments.  The body
    /// is already represented by the canonical fact stream, so replaying that
    /// bounded slice does not add an AST traversal or a second semantic model.
    fn replay_loop_body(
        &mut self,
        body_start: FactId,
        body_end: FactId,
        input: Vec<FlowEnvironment>,
    ) -> Vec<FlowEnvironment> {
        let (Some(start), Some(end)) = (body_start.index(), body_end.index()) else {
            self.run.alternatives_complete = false;
            return Vec::new();
        };
        if start >= end || end > self.stream.facts().len() {
            self.run.alternatives_complete = false;
            return Vec::new();
        }
        let stream = self.stream;
        let previous_mode = self.run.emission_mode;
        self.run.emission_mode = EmissionMode::Replay;
        self.paths = input;
        for i in start..end {
            let fact = &stream.facts()[i];
            self.transfer(fact);
        }
        self.run.emission_mode = previous_mode;
        std::mem::take(&mut self.paths)
    }

    /// Compute the bounded loop back-edge closure.  A semantic state is
    /// canonicalized before it is admitted to the frontier, which prevents
    /// fresh object allocation in repeated iterations from becoming an
    /// unbounded sequence of equivalent alternatives.
    pub(super) fn finish_loop(
        &mut self,
        body_start: FactId,
        body_end: FactId,
        guaranteed: bool,
        baseline: Vec<FlowEnvironment>,
        mut breaks: Vec<FlowEnvironment>,
        mut continues: Vec<FlowEnvironment>,
    ) {
        let mut frontier = std::mem::take(&mut self.paths);
        frontier.append(&mut continues);
        self.join_paths(frontier.clone());
        frontier = std::mem::take(&mut self.paths);

        let mut exits = Vec::new();
        if !guaranteed {
            exits.extend(baseline);
        }
        exits.extend(frontier.iter().copied());
        exits.append(&mut breaks);

        let mut seen = BTreeSet::new();
        for environment in &frontier {
            if !self.charge_operation() {
                break;
            }
            if self.flow_state.restore(*environment) {
                seen.insert(self.flow_state.semantic_snapshot());
            } else {
                self.run.alternatives_complete = false;
            }
        }

        let iteration_limit = self.run.limits.alternative_limit();
        let mut iterations = 0usize;
        while !frontier.is_empty() {
            if iterations >= iteration_limit {
                self.run.alternatives_complete = false;
                break;
            }
            if !self.charge_operation() {
                break;
            }
            iterations += 1;
            self.run.fixed_point_iterations = self.run.fixed_point_iterations.saturating_add(1);
            let break_count = self
                .control
                .iter()
                .rev()
                .find_map(|frame| match frame {
                    ControlFrame::Loop { breaks, .. } => Some(breaks.len()),
                    _ => None,
                })
                .unwrap_or(0);
            let outputs = self.replay_loop_body(body_start, body_end, frontier);
            let mut next = outputs;
            if let Some(ControlFrame::Loop { continues, .. }) = self.control.last_mut() {
                next.append(continues);
            }
            self.join_paths(next);
            let candidate = std::mem::take(&mut self.paths);
            exits.extend(candidate.iter().copied());

            if let Some(ControlFrame::Loop { breaks, .. }) = self.control.last()
                && breaks.len() > break_count
            {
                exits.extend(breaks[break_count..].iter().copied());
            }

            let mut next_frontier = Vec::new();
            for environment in candidate {
                if !self.charge_operation() {
                    break;
                }
                if !self.flow_state.restore(environment) {
                    self.run.alternatives_complete = false;
                    continue;
                }
                if seen.insert(self.flow_state.semantic_snapshot()) {
                    next_frontier.push(environment);
                }
            }
            frontier = next_frontier;
        }

        let Some(ControlFrame::Loop {
            body_start: expected,
            ..
        }) = self.control.last()
        else {
            self.run.alternatives_complete = false;
            return;
        };
        if *expected != body_start {
            self.run.alternatives_complete = false;
            return;
        }
        self.control.pop();
        let mut unique_exits = Vec::with_capacity(exits.len());
        let mut exit_shapes = BTreeSet::new();
        for environment in exits {
            if !self.charge_operation() {
                break;
            }
            if !self.flow_state.restore(environment) {
                self.run.alternatives_complete = false;
                continue;
            }
            if exit_shapes.insert(self.flow_state.semantic_snapshot()) {
                unique_exits.push(environment);
            }
        }
        self.join_paths(unique_exits);
    }

    fn transfer_function(&mut self, boundary: FunctionBoundary) {
        match boundary {
            FunctionBoundary::Enter => {
                let caller = self.paths.clone();
                self.control.push(ControlFrame::Function { caller });
                self.transfer_paths_without_finalization(|projector| {
                    projector.flow_state.clear();
                    projector.run.reachable = true;
                });
            }
            FunctionBoundary::Exit => {
                if let Some(ControlFrame::Function { caller }) = self.control.pop() {
                    self.paths = caller;
                }
            }
        }
    }

    fn transfer_paths_without_finalization(&mut self, transfer: impl Fn(&mut Self)) {
        let incoming = std::mem::take(&mut self.paths);
        let mut outgoing = Vec::with_capacity(incoming.len());
        for environment in incoming {
            if !self.charge_operation() {
                break;
            }
            if !self.flow_state.restore(environment) {
                self.run.alternatives_complete = false;
                continue;
            }
            self.run.reachable = environment.reachable();
            transfer(self);
            if self.run.reachable {
                outgoing.push(self.environment());
            }
        }
        self.paths = outgoing;
        let paths = std::mem::take(&mut self.paths);
        self.join_paths(paths);
    }

    fn transfer_call(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        if !self.run.reachable {
            return;
        }
        let FactPayload::Call {
            receiver,
            target_function,
            args,
            ..
        } = &fact.payload
        else {
            return;
        };
        let cref = CallEffectRef {
            stream: self.stream,
            event: fact.id,
        };
        let effective_args = cref.effective_args().unwrap_or(&[]);
        if let Some(chain) = cref.chain_owned(self.names) {
            self.record_configuration(*receiver, &chain, effective_args, fact.id);
        }
        self.record_sinks(&cref, effective_args, fact.id);
        if let Some(function) = target_function {
            self.record_helper_sink(*function, args, fact.id);
        }
    }

    fn environment(&self) -> FlowEnvironment {
        self.flow_state.capture(self.run.reachable)
    }

    pub(super) fn object_for(&mut self, value: ValueId) -> Option<ObjectId> {
        self.value_aliases(value)
            .into_iter()
            .find_map(|candidate| self.flow_state.object_for(candidate))
    }

    pub(super) fn join_paths(&mut self, mut paths: Vec<FlowEnvironment>) {
        self.observe_alternatives(paths.len());
        paths.retain(FlowEnvironment::is_reachable);
        let mut unique = Vec::with_capacity(paths.len());
        let mut seen_snapshots = Vec::with_capacity(paths.len());
        let mut first = true;
        for path in paths {
            if !self.charge_operation() {
                break;
            }
            if !self.flow_state.restore(path) {
                self.run.alternatives_complete = false;
                continue;
            }
            let snapshot = self.flow_state.semantic_snapshot();
            if seen_snapshots.iter().any(|seen| seen == &snapshot) {
                continue;
            }
            if !first {
                self.run.coalescing_comparisons = self.run.coalescing_comparisons.saturating_add(1);
            }
            first = false;
            seen_snapshots.push(snapshot);
            unique.push(path);
        }
        paths = unique;
        if paths.len() > self.run.limits.alternative_limit() {
            paths.truncate(self.run.limits.alternative_limit());
            self.run.alternatives_complete = false;
        }
        self.paths = paths;
        self.observe_alternatives(self.paths.len());
        self.run.reachable = !self.paths.is_empty();
    }

    fn finalize_pending(&mut self, fact: FactId) {
        let pending = std::mem::take(&mut self.pending);
        for ((rule, flow, event), states) in pending {
            if states.is_empty() {
                continue;
            }
            let matching_paths = states
                .iter()
                .map(|(path, _)| *path)
                .collect::<BTreeSet<_>>();
            let certainty = if self.run.alternatives_complete
                && matching_paths.len() == self.active_path_count
            {
                crate::project::MatchCertainty::Definite
            } else {
                crate::project::MatchCertainty::Possible
            };
            for (_, state) in states {
                self.emit_state_final(&state, event, certainty);
            }
            let _ = (rule, flow, fact);
        }
    }

    pub(super) fn queue_state(&mut self, state: FlowState, event: FactId) {
        self.pending
            .entry((state.flow_id().rule_index().get(), state.flow_id(), event))
            .or_default()
            .push((self.active_path_index, state));
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
        let keys = self
            .flow_state
            .states_for(object)
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        for key in keys {
            let Some(flow) = self.plan.get(key.flow) else {
                continue;
            };
            for (index, requirement) in flow.requirements.iter().enumerate() {
                if let CompiledObjectRequirement::PropertyWrite {
                    property: expected,
                    value: matcher,
                } = requirement
                    && (property.is_none() || property == Some(expected.as_str()))
                {
                    self.flow_state
                        .clear_requirement(key.object, key.flow, index);
                    if value_is_precise
                        && property == Some(expected.as_str())
                        && matcher.matches_flow_value(value)
                    {
                        self.flow_state
                            .record_requirement(key.object, key.flow, index, event);
                    }
                }
            }
            self.emit_if_ready(key.flow, key.object, event);
        }
    }

    fn unbind_value(&mut self, value: ValueId) {
        let mut objects = Vec::new();
        for candidate in self.value_aliases(value) {
            if let Some(object) = self.flow_state.unbind(candidate) {
                objects.push(object);
            }
        }
        for object in objects {
            if !self.flow_state.has_alias_for(object) {
                self.flow_state.remove_states_for(object);
            }
        }
    }

    fn bind_value(&mut self, value: ValueId, object: ObjectId) {
        let candidates = self.value_aliases(value);
        for candidate in candidates {
            self.flow_state.bind(candidate, object);
        }
    }

    fn value_aliases(&mut self, value: ValueId) -> Vec<ValueId> {
        let mut values = vec![value];
        if let Some(resolved) = self.stream.values().resolve_id(value)
            && resolved != value
        {
            values.push(resolved);
        }
        if let Some(slot) = self.stream.values().binding_slot(value) {
            let representative = *self.binding_slots.entry(slot).or_insert(value);
            if !values.contains(&representative) {
                values.push(representative);
            }
        }
        values
    }

    fn invalidate_object(&mut self, value: ValueId) {
        let Some(object) = self.object_for(value) else {
            return;
        };
        self.flow_state.remove_states_for(object);
    }

    fn allocate_object_id(&mut self) -> Option<ObjectId> {
        if self.run.next_object_id >= self.run.limits.object_limit() {
            self.run.object_limit_rejected = true;
            return None;
        }
        let object = ObjectId::new(self.run.next_object_id);
        self.run.next_object_id = self.run.next_object_id.checked_add(1)?;
        Some(object)
    }

    /// Consume the projector and produce a bounded summary of what was used.
    fn into_outcome(self) -> LocalFlowProjectionOutcome {
        let mut flow_evidence = self.flow_evidence;
        flow_evidence.mark_truncated();
        let exhausted = self.run.summary_exhausted
            || self.run.object_limit_rejected
            || self.flow_state.state_limit_rejected()
            || flow_evidence.limit_rejected()
            || self.flow_state.mutation_exhausted()
            || !self.run.alternatives_complete
            || self.trace_arena.is_exhausted();
        LocalFlowProjectionOutcome {
            exhausted,
            objects_used: self.run.next_object_id,
            operations: self.run.operation_budget.used(),
            max_live_alternatives: self.run.max_live_alternatives,
            coalescing_comparisons: self.run.coalescing_comparisons,
            fixed_point_iterations: self.run.fixed_point_iterations,
            trace_heads: self.run.trace_heads,
        }
    }
}
#[cfg(test)]
mod tests;
