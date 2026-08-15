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
        FlowEnvironment, ObjectFlowProjector, PathAdmission, state::FlowSemanticSnapshot,
    },
};

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

    fn incomplete(admission: PathAdmission, complete: &mut bool) -> PathAdmission {
        if matches!(
            admission,
            PathAdmission::Exhausted | PathAdmission::RestoreFailed
        ) {
            *complete = false;
        }
        admission
    }

    /// Admit one environment to a shape set when its semantic shape has not
    /// been seen before.
    fn admit(
        projector: &mut ObjectFlowProjector<'_, '_, '_>,
        seen: &mut BTreeSet<FlowSemanticSnapshot>,
        complete: &mut bool,
        environment: FlowEnvironment,
    ) -> PathAdmission {
        let admission = projector.admit_path(seen, environment);
        Self::incomplete(admission, complete)
    }

    /// Admit each candidate environment to `seen`, keeping only the admitted
    /// ones in order. Admission stops at the first exhausted path.
    fn collect_admitted(
        projector: &mut ObjectFlowProjector<'_, '_, '_>,
        seen: &mut BTreeSet<FlowSemanticSnapshot>,
        complete: &mut bool,
        candidates: Vec<FlowEnvironment>,
    ) -> Vec<FlowEnvironment> {
        let mut admitted = Vec::with_capacity(candidates.len());
        for environment in candidates {
            match Self::admit(projector, seen, complete, environment) {
                PathAdmission::Admitted => admitted.push(environment),
                PathAdmission::Exhausted => break,
                PathAdmission::Duplicate | PathAdmission::RestoreFailed => {}
            }
        }
        admitted
    }

    /// Record a control-frame failure that prevents the fixed point from
    /// completing.
    fn fail(&mut self, projector: &mut ObjectFlowProjector<'_, '_, '_>) {
        self.complete = false;
        projector.mark_control_stack_incomplete();
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
            if Self::admit(projector, &mut self.seen, &mut self.complete, *environment)
                == PathAdmission::Exhausted
            {
                break;
            }
        }
        self.frontier = entrance;

        while !self.frontier.is_empty() {
            if self.iterations >= self.iteration_limit {
                self.complete = false;
                projector.mark_incomplete();
                break;
            }
            if !projector.run.charge_operation() {
                self.complete = false;
                break;
            }
            self.iterations += 1;
            projector.run.fixed_point_iterations =
                projector.run.fixed_point_iterations.saturating_add(1);
            let Ok(break_count) = projector.paths.control.loop_break_count() else {
                self.fail(projector);
                break;
            };
            let inputs = std::mem::take(&mut self.frontier);
            let outputs = projector.replay_loop_body(body_start, body_end, inputs);
            let mut next = outputs;
            let Ok(mut continues) = projector.paths.control.take_loop_continues() else {
                self.fail(projector);
                break;
            };
            next.append(&mut continues);
            projector.join_paths(next);
            let candidate = projector.paths.frontier.take_paths();
            self.exits.extend(candidate.iter().copied());

            let Ok(new_breaks) = projector.paths.control.new_loop_breaks_since(break_count) else {
                self.fail(projector);
                break;
            };
            self.exits.extend(new_breaks);

            self.frontier =
                Self::collect_admitted(projector, &mut self.seen, &mut self.complete, candidate);
        }
    }

    /// Deduplicate the collected exits by semantic shape and return the final
    /// loop outcome.
    pub(super) fn collect_exits(
        &mut self,
        projector: &mut ObjectFlowProjector<'_, '_, '_>,
    ) -> LoopFixedPointOutcome {
        let exits = std::mem::take(&mut self.exits);
        let unique_exits =
            Self::collect_admitted(projector, &mut self.exit_shapes, &mut self.complete, exits);
        LoopFixedPointOutcome {
            exits: unique_exits,
            complete: self.complete,
        }
    }
}
