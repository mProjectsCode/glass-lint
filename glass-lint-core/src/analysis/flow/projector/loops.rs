//! Bounded loop fixed-point state for object-flow projection.
//!
//! A loop is closed over a bounded back-edge frontier. Semantic states are
//! canonicalized before admission, so repeated body replays that allocate
//! fresh projection-local objects still converge on the same aliases and
//! lifecycle requirements.

use std::collections::BTreeSet;

use crate::analysis::{
    facts::FactId,
    flow::projector::{
        AlternativeCompleteness, ControlFrame, FlowEnvironment, FlowStateTable,
        ObjectFlowProjector, ProjectionRunState, state::FlowSemanticSnapshot,
    },
};

/// Whether a loop environment was admitted to the next replay or exit set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopAdmission {
    /// The semantic shape was new and the environment was admitted.
    Admitted,
    /// The shape was already seen; nothing to admit.
    Duplicate,
    /// The environment could not be restored; the fixed point is bounded.
    Unbounded,
    /// The operation budget is exhausted; admission must stop.
    Exhausted,
}

/// Outcome of a bounded loop fixed-point computation.
#[derive(Debug)]
pub(super) struct LoopFixedPointOutcome {
    /// Exits leaving the loop, deduplicated by semantic shape.
    pub(super) exits: Vec<FlowEnvironment>,
    /// Whether the fixed point completed within the operation budget and the
    /// alternative limit.
    pub(super) complete: bool,
}

/// Bounded fixed-point state for one loop.
///
/// Owns the replay frontier, the admitted snapshots, the collected exits, and
/// the convergence bounds. The projector method only orchestrates the entrance
/// coalescing, the loop-frame hand-off, and the final join.
pub(super) struct LoopFixedPoint {
    /// Environments waiting for the next loop-body replay.
    frontier: Vec<FlowEnvironment>,
    /// Semantic shapes already admitted; a repeated shape has converged.
    seen: BTreeSet<FlowSemanticSnapshot>,
    /// Environments leaving the loop on any path, awaiting deduplication.
    exits: Vec<FlowEnvironment>,
    /// Semantic shapes already collected as loop exits.
    exit_shapes: BTreeSet<FlowSemanticSnapshot>,
    /// Complete body replays performed so far.
    iterations: usize,
    /// Maximum replays before the fixed point is considered non-convergent.
    iteration_limit: usize,
    /// Whether budget, restore, or iteration bounds kept the fixed point from
    /// completing.
    complete: bool,
}

impl LoopFixedPoint {
    pub(super) fn start(
        input: Vec<FlowEnvironment>,
        baseline: Vec<FlowEnvironment>,
        guaranteed: bool,
        mut breaks: Vec<FlowEnvironment>,
        iteration_limit: usize,
    ) -> Self {
        let mut exits = Vec::new();
        if !guaranteed {
            exits.extend(baseline);
        }
        exits.extend(input.iter().copied());
        exits.append(&mut breaks);
        Self {
            frontier: input,
            seen: BTreeSet::new(),
            exits,
            exit_shapes: BTreeSet::new(),
            iterations: 0,
            iteration_limit,
            complete: true,
        }
    }

    fn admit_into(
        shapes: &mut BTreeSet<FlowSemanticSnapshot>,
        flow_state: &mut FlowStateTable,
        run: &mut ProjectionRunState,
        environment: FlowEnvironment,
    ) -> LoopAdmission {
        if !run.charge_operation() {
            return LoopAdmission::Exhausted;
        }
        if !flow_state.restore(environment) {
            run.alternatives_complete = AlternativeCompleteness::Incomplete;
            return LoopAdmission::Unbounded;
        }
        if shapes.insert(flow_state.semantic_snapshot()) {
            LoopAdmission::Admitted
        } else {
            LoopAdmission::Duplicate
        }
    }

    /// Admit an environment to the next replay when its semantic shape has not
    /// been seen before.
    fn admit_replay(
        &mut self,
        flow_state: &mut FlowStateTable,
        run: &mut ProjectionRunState,
        environment: FlowEnvironment,
    ) -> LoopAdmission {
        let admission = Self::admit_into(&mut self.seen, flow_state, run, environment);
        if matches!(
            admission,
            LoopAdmission::Exhausted | LoopAdmission::Unbounded
        ) {
            self.complete = false;
        }
        admission
    }

    /// Admit one exit environment to the final exit set.
    fn admit_exit(
        &mut self,
        flow_state: &mut FlowStateTable,
        run: &mut ProjectionRunState,
        environment: FlowEnvironment,
    ) -> LoopAdmission {
        let admission = Self::admit_into(&mut self.exit_shapes, flow_state, run, environment);
        if matches!(
            admission,
            LoopAdmission::Exhausted | LoopAdmission::Unbounded
        ) {
            self.complete = false;
        }
        admission
    }

    /// Drive the fixed point until the replay frontier converges or the bounds
    /// are exhausted, collecting every exit path as it goes.
    pub(super) fn converge(
        &mut self,
        projector: &mut ObjectFlowProjector<'_, '_, '_>,
        body_start: FactId,
        body_end: FactId,
    ) {
        let entrance = std::mem::take(&mut self.frontier);
        for environment in &entrance {
            if self.admit_replay(&mut projector.flow_state, &mut projector.run, *environment)
                == LoopAdmission::Exhausted
            {
                break;
            }
        }
        self.frontier = entrance;

        while !self.frontier.is_empty() {
            if self.iterations >= self.iteration_limit {
                self.complete = false;
                projector.run.alternatives_complete = AlternativeCompleteness::Incomplete;
                break;
            }
            if !projector.run.charge_operation() {
                self.complete = false;
                break;
            }
            self.iterations += 1;
            projector.run.fixed_point_iterations =
                projector.run.fixed_point_iterations.saturating_add(1);
            let break_count = projector
                .control
                .iter()
                .rev()
                .find_map(|frame| match frame {
                    ControlFrame::Loop { breaks, .. } => Some(breaks.len()),
                    _ => None,
                })
                .unwrap_or(0);
            let inputs = std::mem::take(&mut self.frontier);
            let outputs = projector.replay_loop_body(body_start, body_end, inputs);
            let mut next = outputs;
            if let Some(ControlFrame::Loop { continues, .. }) = projector.control.last_mut() {
                next.append(continues);
            }
            projector.join_paths(next);
            let candidate = projector.frontier.take();
            self.exits.extend(candidate.iter().copied());

            if let Some(ControlFrame::Loop { breaks, .. }) = projector.control.last()
                && breaks.len() > break_count
            {
                self.exits.extend(breaks[break_count..].iter().copied());
            }

            let mut next_frontier = Vec::new();
            for environment in candidate {
                match self.admit_replay(&mut projector.flow_state, &mut projector.run, environment)
                {
                    LoopAdmission::Admitted => next_frontier.push(environment),
                    LoopAdmission::Exhausted => break,
                    LoopAdmission::Duplicate | LoopAdmission::Unbounded => {}
                }
            }
            self.frontier = next_frontier;
        }
    }

    /// Deduplicate the collected exits by semantic shape and return the final
    /// loop outcome.
    pub(super) fn collect_exits(
        &mut self,
        flow_state: &mut FlowStateTable,
        run: &mut ProjectionRunState,
    ) -> LoopFixedPointOutcome {
        let mut unique_exits = Vec::with_capacity(self.exits.len());
        let exits = std::mem::take(&mut self.exits);
        for environment in exits {
            match self.admit_exit(flow_state, run, environment) {
                LoopAdmission::Admitted => unique_exits.push(environment),
                LoopAdmission::Exhausted => break,
                LoopAdmission::Duplicate | LoopAdmission::Unbounded => {}
            }
        }
        LoopFixedPointOutcome {
            exits: unique_exits,
            complete: self.complete,
        }
    }
}
