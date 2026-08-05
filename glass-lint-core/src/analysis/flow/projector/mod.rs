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
mod loops;
mod state;
mod transfer;

use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::{Budget, NameTable};
use loops::LoopFixedPoint;
use state::{
    AbruptExit, ControlFrame, ControlStack, FlowEnvironment, FlowEvidence, FlowStateTable,
    PropertyWriteUpdate,
};

use crate::{
    analysis::{
        facts::{
            CallArgInfo, ControlKind, FactId, FactPayload, FactStream, Frozen, FunctionBoundary,
        },
        flow::{effect::FunctionEffects, planning::BoundFlowPlan, summary::FunctionSummaries},
        model::flow::{FlowId, FlowLimits, FlowState},
        trace::TraceArena,
        value::{BindingSlot, ObjectId, ValueId},
    },
    api::{
        classification::{ClassificationEvidence, MatchKind, RuleEvidenceTable, RuleIndex},
        compiler::CompiledObjectFlow,
    },
    project::{MatchCertainty, ModuleId},
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
        evidence,
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
    control: ControlStack,
    /// Correlated checkpoint-backed alternatives and fact-local replay cursor.
    frontier: PathFrontier,
    /// Fact-local witnesses are finalized after every reaching alternative has
    /// seen the sink or requirement event.
    pending: PendingFlowStates,
    /// Stable representative value for each lexical binding slot. Binding
    /// versions differ at joins, but the slot remains the same variable.
    binding_slots: BTreeMap<BindingSlot, ValueId>,
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
    alternatives_complete: AlternativeCompleteness,
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

#[derive(Debug)]
struct PathFrontier {
    paths: Vec<FlowEnvironment>,
    active_count: usize,
    active_index: usize,
}

/// The active frontier paths at a fact boundary, keyed by their per-transfer
/// indices. Pending states are definite only when every active path queued a
/// matching state.
#[derive(Debug, Clone, Copy)]
struct ActivePaths {
    count: usize,
}

impl ActivePaths {
    fn new(count: usize) -> Self {
        Self { count }
    }

    fn len(self) -> usize {
        self.count
    }

    fn contains(self, path_index: usize) -> bool {
        path_index < self.count
    }
}

#[derive(Debug, Default)]
struct PendingFlowStates {
    values: BTreeMap<PendingFlowKey, Vec<PendingState>>,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct PendingFlowKey {
    flow: FlowId,
    event: FactId,
}

#[derive(Debug)]
struct PendingState {
    path_index: usize,
    state: FlowState,
}

/// One drained pending group with the completing event and the certainty
/// derived from active-path coverage.
#[derive(Debug)]
struct PendingFlowStateFinal {
    event: FactId,
    certainty: MatchCertainty,
    states: Vec<PendingState>,
}

impl PendingFlowStates {
    fn finalize(
        &mut self,
        active_paths: ActivePaths,
        alternatives_complete: AlternativeCompleteness,
    ) -> Vec<PendingFlowStateFinal> {
        let values = std::mem::take(&mut self.values);
        let mut finalized = Vec::with_capacity(values.len());
        for (key, states) in values {
            if states.is_empty() {
                continue;
            }
            let matching_paths = states
                .iter()
                .map(|pending| pending.path_index)
                .collect::<BTreeSet<_>>();
            let definite = alternatives_complete.is_complete()
                && matching_paths.len() == active_paths.len()
                && matching_paths
                    .iter()
                    .all(|path| active_paths.contains(*path));
            let certainty = if definite {
                MatchCertainty::Definite
            } else {
                MatchCertainty::Possible
            };
            finalized.push(PendingFlowStateFinal {
                event: key.event,
                certainty,
                states,
            });
        }
        finalized
    }

    fn entry(&mut self, key: PendingFlowKey) -> &mut Vec<PendingState> {
        self.values.entry(key).or_default()
    }
}

impl PathFrontier {
    fn initial() -> Self {
        Self {
            paths: vec![FlowEnvironment::initial()],
            active_count: 0,
            active_index: 0,
        }
    }

    fn active_paths(&self) -> ActivePaths {
        ActivePaths::new(self.active_count)
    }

    fn take(&mut self) -> Vec<FlowEnvironment> {
        std::mem::take(&mut self.paths)
    }

    fn set(&mut self, paths: Vec<FlowEnvironment>) {
        self.paths = paths;
    }

    fn len(&self) -> usize {
        self.paths.len()
    }

    fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlternativeCompleteness {
    Complete,
    Incomplete,
}

impl AlternativeCompleteness {
    fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }
}

impl ProjectionRunState {
    fn new(limits: FlowLimits, summary_exhausted: bool) -> Self {
        Self {
            limits,
            next_object_id: 0,
            object_limit_rejected: false,
            alternatives_complete: AlternativeCompleteness::Complete,
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

    fn charge_operation(&mut self) -> bool {
        if self.operation_budget.try_push() {
            true
        } else {
            self.alternatives_complete = AlternativeCompleteness::Incomplete;
            false
        }
    }
}

struct ObjectFlowProjectorInput<'rules, 'stream, 'arena> {
    stream: &'stream FactStream<Frozen>,
    names: &'stream NameTable,
    plan: BoundFlowPlan<'rules>,
    helpers: FunctionSummaries<'stream>,
    evidence: &'stream mut RuleEvidenceTable,
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
            control: ControlStack::default(),
            frontier: PathFrontier::initial(),
            pending: PendingFlowStates::default(),
            binding_slots: BTreeMap::new(),
            module_id,
            trace_arena,
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
        let incoming = self.frontier.take();
        if incoming.is_empty() {
            return;
        }
        self.frontier.active_count = incoming.len();
        let mut outgoing = Vec::with_capacity(incoming.len());
        for (path_index, environment) in incoming.into_iter().enumerate() {
            if !self.run.charge_operation() {
                break;
            }
            if !self.flow_state.restore(environment) {
                self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
                continue;
            }
            self.run.reachable = environment.is_reachable();
            self.frontier.active_index = path_index;
            self.transfer_fact(fact);
            if self.run.reachable {
                outgoing.push(self.environment());
            }
        }
        self.frontier.set(outgoing);
        self.observe_alternatives(self.frontier.len());
        self.finalize_pending();
        let paths = self.frontier.take();
        self.join_paths(paths);
        self.frontier.active_count = 0;
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
    pub(super) fn replay_loop_body(
        &mut self,
        body_start: FactId,
        body_end: FactId,
        input: Vec<FlowEnvironment>,
    ) -> Vec<FlowEnvironment> {
        let (Some(start), Some(end)) = (body_start.index(), body_end.index()) else {
            self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
            return Vec::new();
        };
        if start >= end || end > self.stream.facts().len() {
            self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
            return Vec::new();
        }
        let stream = self.stream;
        let previous_mode = self.run.emission_mode;
        self.run.emission_mode = EmissionMode::Replay;
        self.frontier.set(input);
        for i in start..end {
            let fact = &stream.facts()[i];
            self.transfer(fact);
        }
        self.run.emission_mode = previous_mode;
        self.frontier.take()
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
        breaks: Vec<FlowEnvironment>,
        mut continues: Vec<FlowEnvironment>,
    ) {
        let mut entrance = self.frontier.take();
        entrance.append(&mut continues);
        self.join_paths(entrance.clone());
        let entrance = self.frontier.take();

        let mut fixed_point = LoopFixedPoint::start(
            entrance,
            baseline,
            guaranteed,
            breaks,
            self.run.limits.alternative_limit(),
        );
        fixed_point.converge(&mut *self, body_start, body_end);

        if !self.control.pop_loop(body_start) {
            self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
            return;
        }

        let outcome = fixed_point.collect_exits(&mut self.flow_state, &mut self.run);
        if !outcome.complete {
            self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
        }
        self.join_paths(outcome.exits);
    }

    fn transfer_function(&mut self, boundary: FunctionBoundary) {
        match boundary {
            FunctionBoundary::Enter => {
                let caller = self.frontier.paths.clone();
                self.control.push(ControlFrame::Function { caller });
                self.transfer_paths_without_finalization(|projector| {
                    projector.flow_state.clear();
                    projector.run.reachable = true;
                });
            }
            FunctionBoundary::Exit => {
                if let Some(caller) = self.control.pop_function() {
                    self.frontier.set(caller);
                }
            }
        }
    }

    fn transfer_paths_without_finalization(&mut self, transfer: impl Fn(&mut Self)) {
        let incoming = self.frontier.take();
        let mut outgoing = Vec::with_capacity(incoming.len());
        for environment in incoming {
            if !self.run.charge_operation() {
                break;
            }
            if !self.flow_state.restore(environment) {
                self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
                continue;
            }
            self.run.reachable = environment.is_reachable();
            transfer(self);
            if self.run.reachable {
                outgoing.push(self.environment());
            }
        }
        self.frontier.set(outgoing);
        let paths = self.frontier.take();
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
        let cref = self.stream.call_effect(fact.id);
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
        let aliases = self.value_aliases(value);
        self.flow_state.object_for_any(&aliases)
    }

    pub(super) fn join_paths(&mut self, mut paths: Vec<FlowEnvironment>) {
        self.observe_alternatives(paths.len());
        paths.retain(FlowEnvironment::is_reachable);
        let mut unique = Vec::with_capacity(paths.len());
        let mut seen_snapshots = Vec::with_capacity(paths.len());
        let mut first = true;
        for path in paths {
            if !self.run.charge_operation() {
                break;
            }
            if !self.flow_state.restore(path) {
                self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
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
            self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
        }
        self.frontier.set(paths);
        self.observe_alternatives(self.frontier.len());
        self.run.reachable = !self.frontier.is_empty();
    }

    fn finalize_pending(&mut self) {
        let finalized = self
            .pending
            .finalize(self.frontier.active_paths(), self.run.alternatives_complete);
        for record in finalized {
            for pending in record.states {
                self.emit_state_final(&pending.state, record.event, record.certainty);
            }
        }
    }

    pub(super) fn queue_state(&mut self, state: FlowState, event: FactId) {
        self.pending
            .entry(PendingFlowKey {
                flow: state.flow_id(),
                event,
            })
            .push(PendingState {
                path_index: self.frontier.active_index,
                state,
            });
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
                self.plan
                    .requirements_with_indices(flow_id)
                    .filter_map(|(index, requirement)| {
                        let (expected, matcher) = requirement.property_write()?;
                        (property.is_none() || property == Some(expected.as_str())).then(|| {
                            PropertyWriteUpdate::new(
                                index,
                                value_is_precise
                                    && property == Some(expected.as_str())
                                    && matcher.matches_flow_value(value),
                            )
                        })
                    })
                    .collect()
            });
        for flow in updated {
            self.emit_if_ready(flow, object, event);
        }
    }

    fn unbind_value(&mut self, value: ValueId) {
        let aliases = self.value_aliases(value);
        self.flow_state.unbind_aliases(&aliases);
    }

    fn bind_value(&mut self, value: ValueId, object: ObjectId) {
        let aliases = self.value_aliases(value);
        self.flow_state.bind_aliases(&aliases, object);
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
        let aliases = self.value_aliases(value);
        self.flow_state.invalidate_aliases(&aliases);
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
            || !self.run.alternatives_complete.is_complete()
            || self.trace_arena.is_exhausted();
        LocalFlowProjectionOutcome {
            exhausted,
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
