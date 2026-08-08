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
    AbruptExit, ControlFrame, ControlStack, FlowEnvironment, FlowEvidence, FlowSemanticSnapshot,
    FlowStateTable, PropertyWriteUpdate,
};

use crate::{
    analysis::{
        facts::{
            CallArgInfo, ControlKind, FactId, FactPayload, FactStream, Frozen, FunctionBoundary,
        },
        flow::{
            FlowCompletion, FlowCompletionReason, effect::FunctionEffects, planning::BoundFlowPlan,
            summary::FunctionSummaries,
        },
        model::{
            flow::{FlowId, FlowLimits, FlowState},
            scope::BindingSlot,
            value::{ObjectId, ValueId},
        },
        trace::TraceArena,
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

/// Result of admitting one environment into a semantic-path set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PathAdmission {
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

impl FlowCompletion {
    fn from_sources(
        run: &ProjectionRunState,
        flow_state: &FlowStateTable,
        flow_evidence: &FlowEvidence<'_>,
        trace_arena: &TraceArena,
    ) -> Self {
        let mut completion = Self::default();
        completion.mark_if(FlowCompletionReason::Summary, run.summary_exhausted);
        completion.mark_if(FlowCompletionReason::ObjectLimit, run.object_limit_rejected);
        completion.mark_if(
            FlowCompletionReason::StateLimit,
            flow_state.state_limit_rejected(),
        );
        completion.mark_if(
            FlowCompletionReason::EvidenceLimit,
            flow_evidence.limit_rejected(),
        );
        completion.mark_if(
            FlowCompletionReason::MutationLog,
            flow_state.mutation_exhausted(),
        );
        completion.mark_if(
            FlowCompletionReason::Alternatives,
            !run.alternatives_complete.is_complete(),
        );
        completion.mark_if(FlowCompletionReason::TraceArena, trace_arena.is_exhausted());
        completion
    }

    fn mark_if(&mut self, reason: FlowCompletionReason, exhausted: bool) {
        if exhausted {
            self.mark(reason);
        }
    }
}

/// Exhaustion state and bounded counters returned by local flow projection.
#[derive(Debug, Clone, Copy, Default)]
pub(in crate::analysis) struct LocalFlowProjectionOutcome {
    completion: FlowCompletion,
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

impl LocalFlowProjectionOutcome {
    /// Whether any local projection resource was exhausted.
    pub(in crate::analysis) fn is_exhausted(&self) -> bool {
        self.completion.is_incomplete()
    }
}

#[derive(Clone, Copy)]
pub(in crate::analysis) struct FlowProjectionRule<'a> {
    rule_index: RuleIndex,
    root_index: usize,
    flow: &'a CompiledObjectFlow,
}

impl<'a> FlowProjectionRule<'a> {
    pub(in crate::analysis) fn new(
        rule_index: RuleIndex,
        root_index: usize,
        flow: &'a CompiledObjectFlow,
    ) -> Self {
        Self {
            rule_index,
            root_index,
            flow,
        }
    }

    fn as_bound_flow(self) -> (RuleIndex, usize, &'a CompiledObjectFlow) {
        (self.rule_index, self.root_index, self.flow)
    }
}

/// Push flow evidence directly into an externally-owned per-rule vec,
/// avoiding a separate evidence matrix allocation alongside the caller's.
/// Returns the exhaustion state and bounded counters for the caller.
pub(in crate::analysis) fn collect_into(
    stream: &FactStream<Frozen>,
    effects: &FunctionEffects,
    rules: &[FlowProjectionRule<'_>],
    evidence: &mut RuleEvidenceTable,
    limits: FlowLimits,
    module_id: ModuleId,
    trace_arena: &mut TraceArena,
) -> LocalFlowProjectionOutcome {
    if rules.is_empty() {
        return LocalFlowProjectionOutcome::default();
    }
    let names = stream.names();
    let bound_rules: Vec<_> = rules
        .iter()
        .copied()
        .map(FlowProjectionRule::as_bound_flow)
        .collect();
    let plan = BoundFlowPlan::new(&bound_rules, names);
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
    let mut evidence = RuleEvidenceTable::new_for_test(rule_count);
    let outcome = collect_into(
        stream,
        effects,
        &rules
            .iter()
            .copied()
            .map(|(rule_index, root_index, flow)| {
                FlowProjectionRule::new(rule_index, root_index, flow)
            })
            .collect::<Vec<_>>(),
        &mut evidence,
        limits,
        module_id,
        trace_arena,
    );
    (evidence, outcome)
}

#[derive(Debug)]
struct ObjectFlowProjector<'rules, 'stream, 'arena> {
    /// Immutable canonical inputs remain separate from state mutated while a
    /// file is projected. In particular, the projector must never inspect
    /// the AST or reconstruct resolution decisions.
    inputs: ProjectionInputs<'rules, 'stream>,
    /// Evidence is grouped and deduplicated by the flow-specific evidence
    /// owner.
    flow_evidence: FlowEvidence<'stream>,
    /// Each value identity and live object-flow state are owned together.
    flow_state: FlowStateTable,
    /// Bounded lifecycle, allocation, emission, and outcome state for one run.
    run: ProjectionRunState,
    /// Path alternatives, control frames, pending certainty, and lexical
    /// binding representatives move together through one private machine.
    paths: ProjectionPathMachine,
    /// Shared trace arena for interning evidence trace nodes.
    trace_arena: &'arena mut TraceArena,
}

#[derive(Debug)]
struct ProjectionPathMachine {
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
}

impl ProjectionPathMachine {
    fn initial() -> Self {
        Self {
            control: ControlStack::default(),
            frontier: PathFrontier::initial(),
            pending: PendingFlowStates::default(),
            binding_slots: BTreeMap::new(),
        }
    }
}

#[derive(Debug)]
struct ProjectionInputs<'rules, 'stream> {
    stream: &'stream FactStream<Frozen>,
    names: &'stream NameTable,
    plan: BoundFlowPlan<'rules>,
    helpers: FunctionSummaries<'stream>,
    /// Call results are indexed once so later assignments can start a flow
    /// without rescanning the fact stream.
    calls_by_result: BTreeMap<ValueId, FactId>,
    /// Module being projected, used to qualify trace events.
    module_id: ModuleId,
}

impl<'rules, 'stream> ProjectionInputs<'rules, 'stream> {
    fn new(
        stream: &'stream FactStream<Frozen>,
        names: &'stream NameTable,
        plan: BoundFlowPlan<'rules>,
        helpers: FunctionSummaries<'stream>,
        module_id: ModuleId,
    ) -> Self {
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
            module_id,
        }
    }
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
    next_batch: u64,
    active_batch: Option<ActivePaths>,
    active_path: Option<PathToken>,
}

/// The active frontier paths at a fact boundary, keyed by their per-transfer
/// indices. Pending states are definite only when every active path queued a
/// matching state.
#[derive(Debug, Clone, Copy)]
struct ActivePaths {
    generation: u64,
    count: usize,
}

impl ActivePaths {
    fn token(self, index: usize) -> Option<PathToken> {
        (index < self.count).then_some(PathToken {
            generation: self.generation,
            index,
        })
    }

    fn len(self) -> usize {
        self.count
    }

    fn contains(self, path: PathToken) -> bool {
        if path.generation != self.generation {
            return false;
        }
        path.index < self.count
    }
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct PathToken {
    generation: u64,
    index: usize,
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
    path: PathToken,
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
                .map(|pending| pending.path)
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
            next_batch: 0,
            active_batch: None,
            active_path: None,
        }
    }

    fn begin_batch(&mut self, count: usize) -> ActivePaths {
        self.next_batch = self.next_batch.saturating_add(1);
        let batch = ActivePaths {
            generation: self.next_batch,
            count,
        };
        self.active_batch = Some(batch);
        self.active_path = None;
        batch
    }

    fn select_path(&mut self, index: usize) -> bool {
        self.active_path = self.active_batch.and_then(|batch| batch.token(index));
        self.active_path.is_some()
    }

    fn active_paths(&self) -> Option<ActivePaths> {
        self.active_batch
    }

    fn active_path(&self) -> Option<PathToken> {
        self.active_path
    }

    fn end_batch(&mut self) {
        self.active_path = None;
        self.active_batch = None;
    }

    fn take_paths(&mut self) -> Vec<FlowEnvironment> {
        std::mem::take(&mut self.paths)
    }

    fn replace_paths(&mut self, paths: Vec<FlowEnvironment>) {
        self.paths = paths;
    }

    fn snapshot_paths(&self) -> Vec<FlowEnvironment> {
        self.paths.clone()
    }

    fn append_paths(&mut self, paths: impl IntoIterator<Item = FlowEnvironment>) {
        self.paths.extend(paths);
    }

    fn path_count(&self) -> usize {
        self.paths.len()
    }

    fn has_paths(&self) -> bool {
        !self.paths.is_empty()
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
            operation_budget: Budget::new(limits.operation_limit()),
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
        Self {
            inputs: ProjectionInputs::new(stream, names, plan, helpers, module_id),
            flow_evidence: FlowEvidence::new(evidence),
            flow_state: FlowStateTable::new(limits.state_limit(), limits.mutation_limit()),
            run: ProjectionRunState::new(limits, summary_exhausted),
            paths: ProjectionPathMachine::initial(),
            trace_arena,
        }
    }

    fn observe_alternatives(&mut self, count: usize) {
        self.run.max_live_alternatives = self.run.max_live_alternatives.max(count);
    }

    pub(super) fn mark_control_stack_incomplete(&mut self) {
        self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
    }

    fn transfer(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        match &fact.payload {
            FactPayload::Function { boundary, .. } => self.transfer_function(*boundary),
            FactPayload::Control { kind, region } => {
                self.transfer_control(*kind, *region, fact.id);
            }
            FactPayload::Return { region, .. } => {
                self.transfer_control(ControlKind::Return, *region, fact.id);
            }
            _ => self.transfer_paths(fact),
        }
    }

    fn transfer_paths(&mut self, fact: &crate::analysis::facts::SemanticFact) {
        let incoming = self.paths.frontier.take_paths();
        if incoming.is_empty() {
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
            self.transfer_fact(fact);
            if self.run.reachable {
                outgoing.push(self.environment());
            }
        }
        self.paths.frontier.replace_paths(outgoing);
        self.observe_alternatives(self.paths.frontier.path_count());
        self.finalize_pending();
        let paths = self.paths.frontier.take_paths();
        self.join_paths(paths);
        self.paths.frontier.end_batch();
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
                let static_string = self.inputs.stream.values().static_string(*value);
                self.record_property_write(
                    *receiver,
                    property.and_then(|id| self.inputs.stream.resolve_name(id)),
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
        if start >= end || end > self.inputs.stream.facts().len() {
            self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
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
    pub(super) fn finish_loop(
        &mut self,
        body_start: FactId,
        body_end: FactId,
        guaranteed: bool,
        baseline: Vec<FlowEnvironment>,
        breaks: Vec<FlowEnvironment>,
        mut continues: Vec<FlowEnvironment>,
    ) {
        let mut entrance = self.paths.frontier.take_paths();
        entrance.append(&mut continues);
        self.join_paths(entrance.clone());
        let entrance = self.paths.frontier.take_paths();

        let mut fixed_point = LoopFixedPoint::start(
            entrance,
            baseline,
            guaranteed,
            breaks,
            self.run.limits.alternative_limit(),
        );
        fixed_point.converge(&mut *self, body_start, body_end);

        if self.paths.control.pop_loop(body_start).is_err() {
            self.mark_control_stack_incomplete();
            return;
        }

        let outcome = fixed_point.collect_exits(self);
        if !outcome.complete {
            self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
        }
        self.join_paths(outcome.exits);
    }

    fn transfer_function(&mut self, boundary: FunctionBoundary) {
        match boundary {
            FunctionBoundary::Enter => {
                let caller = self.paths.frontier.snapshot_paths();
                self.paths.control.push(ControlFrame::Function { caller });
                self.transfer_paths_without_finalization(|projector| {
                    projector.flow_state.clear();
                    projector.run.reachable = true;
                });
            }
            FunctionBoundary::Exit => match self.paths.control.pop_function() {
                Ok(caller) => self.paths.frontier.replace_paths(caller),
                Err(_) => self.mark_control_stack_incomplete(),
            },
        }
    }

    fn transfer_paths_without_finalization(&mut self, transfer: impl Fn(&mut Self)) {
        let incoming = self.paths.frontier.take_paths();
        let mut outgoing = Vec::with_capacity(incoming.len());
        for environment in incoming {
            match self.restore_path(environment) {
                PathRestoration::Exhausted => break,
                PathRestoration::Failed => continue,
                PathRestoration::Ready => {}
            }
            transfer(self);
            if self.run.reachable {
                outgoing.push(self.environment());
            }
        }
        self.paths.frontier.replace_paths(outgoing);
        let paths = self.paths.frontier.take_paths();
        self.join_paths(paths);
    }

    fn restore_path(&mut self, environment: FlowEnvironment) -> PathRestoration {
        if !self.run.charge_operation() {
            return PathRestoration::Exhausted;
        }
        if !self.flow_state.restore(environment) {
            self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
            return PathRestoration::Failed;
        }
        self.run.reachable = environment.is_reachable();
        PathRestoration::Ready
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
        let cref = self.inputs.stream.call_effect(fact.id);
        let Some(shape) = cref.shape() else {
            if let Some(function) = target_function {
                self.record_helper_sink(*function, args, fact.id);
            }
            return;
        };
        let effective_args = shape.effective_args();
        if let Some(chain) = shape.chain_owned(self.inputs.stream, self.inputs.names) {
            self.record_configuration(*receiver, &chain, effective_args, fact.id);
        }
        self.record_sinks(&shape, effective_args, fact.id);
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
            self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
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
            self.run.alternatives_complete = AlternativeCompleteness::Incomplete;
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
        for record in finalized {
            for pending in record.states {
                self.emit_state_final(&pending.state, record.event, record.certainty);
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
                    .map(|match_result| {
                        PropertyWriteUpdate::new(match_result.index(), match_result.value_matches())
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
        let completion = FlowCompletion::from_sources(
            &self.run,
            &self.flow_state,
            &flow_evidence,
            self.trace_arena,
        );
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
#[cfg(test)]
mod tests;
