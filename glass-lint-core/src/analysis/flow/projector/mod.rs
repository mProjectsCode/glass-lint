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
mod driver;
mod evidence;
mod history;
mod loops;
mod state;
mod transfer;

use std::collections::{BTreeMap, BTreeSet};

pub(in crate::analysis::flow::projector) use driver::{
    EmissionMode, ObjectFlowProjectorInput, PathAdmission,
};
use glass_lint_datastructures::{Budget, NameTable};
use state::{
    AbruptExit, ControlFrame, ControlStack, FlowEnvironment, FlowEvidence, FlowSemanticSnapshot,
    FlowStateTable, PropertyWriteUpdate,
};

#[cfg(test)]
use crate::api::{classification::RuleIndex, compiler::CompiledObjectFlow};
use crate::{
    analysis::{
        facts::{CallArgInfo, ControlKind, FactId, FactPayload, FactStream, Frozen},
        flow::{
            FlowCompletion, FlowCompletionReason,
            effect::FunctionEffects,
            planning::{BoundFlowPlan, BoundLifecycleRoot},
            summary::FunctionSummaries,
        },
        model::{
            flow::{FlowId, FlowLimits, FlowState},
            scope::BindingSlot,
            value::{FlowObjectId, ValueId},
        },
        trace::TraceArena,
    },
    api::classification::{ClassificationEvidence, MatchKind, RuleEvidenceTable},
    project::{MatchCertainty, ModuleId},
};

impl FlowCompletion {
    pub(in crate::analysis::flow::projector) fn from_sources(
        run: &ProjectionRunState,
        flow_state: &FlowStateTable,
        flow_evidence: &FlowEvidence<'_>,
        trace_arena: &TraceArena,
    ) -> Self {
        let mut completion = Self::default();
        completion.merge(run.completion);
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

/// Push flow evidence directly into an externally-owned per-rule vec,
/// avoiding a separate evidence matrix allocation alongside the caller's.
/// Returns the exhaustion state and bounded counters for the caller.
pub(in crate::analysis) fn collect_into(
    stream: &FactStream<Frozen>,
    effects: &FunctionEffects,
    rules: &[BoundLifecycleRoot<'_>],
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
    let completion = helpers.completion();
    let mut projector = ObjectFlowProjector::new(ObjectFlowProjectorInput {
        stream,
        names,
        plan,
        helpers,
        evidence,
        limits,
        completion,
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
                BoundLifecycleRoot::new(rule_index, root_index, flow)
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
                FactPayload::Call(call) => Some((call.result(), fact.id)),
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
    completion: FlowCompletion,
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
    fn new(limits: FlowLimits, completion: FlowCompletion) -> Self {
        Self {
            limits,
            next_object_id: 0,
            object_limit_rejected: false,
            alternatives_complete: AlternativeCompleteness::Complete,
            reachable: true,
            completion,
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

#[cfg(test)]
mod tests;
