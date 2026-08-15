//! Control-path state and environment algebra for object-flow projection.
//!
//! Environments are immutable snapshots at branch boundaries. The projector
//! retains a bounded collection of these checkpoints so aliases and lifecycle
//! requirements stay correlated across control-flow merges.

use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::HistoryTransition;

use crate::{
    analysis::{
        facts::{ControlRegionId, FactId},
        flow::projector::history::{Checkpoint, InverseDelta, ReportEvidenceKey},
        model::flow::{FlowState, FlowStateKey},
    },
    api::classification::{ClassificationEvidence, RuleEvidenceTable, RuleIndex},
};

mod tables;

use tables::AliasTable;
pub(super) use tables::{
    FlowEnvironment, FlowSemanticSnapshot, FlowStateTable, PropertyWriteUpdate, StateAdmission,
};

impl InverseDelta {
    fn apply(
        &self,
        direction: HistoryTransition,
        aliases: &mut AliasTable,
        states: &mut BTreeMap<FlowStateKey, FlowState>,
    ) {
        let undo = matches!(direction, HistoryTransition::Undo);
        match self {
            Self::AliasInsert(value, object) => {
                if undo {
                    aliases.remove(*value);
                } else {
                    aliases.set(*value, *object);
                }
            }
            Self::AliasUpdate(value, old, new) => {
                aliases.set(*value, if undo { *old } else { *new });
            }
            Self::AliasRemove(value, object) => {
                if undo {
                    aliases.set(*value, *object);
                } else {
                    aliases.remove(*value);
                }
            }
            Self::StateInsert(key, state) => {
                if undo {
                    states.remove(key);
                } else {
                    states.insert(*key, (**state).clone());
                }
            }
            Self::StateUpdate(key, old, new) => {
                states.insert(
                    *key,
                    if undo {
                        (**old).clone()
                    } else {
                        (**new).clone()
                    },
                );
            }
            Self::StateRemove(key, state) => {
                if undo {
                    states.insert(*key, (**state).clone());
                } else {
                    states.remove(key);
                }
            }
            Self::RequirementInsert(key, index, event) => {
                if let Some(state) = states.get_mut(key) {
                    if undo {
                        state.remove_requirement_event(*index, *event);
                    } else {
                        state.record_requirement(*index, *event);
                    }
                }
            }
            Self::RequirementRemove(key, index, events) => {
                if let Some(state) = states.get_mut(key) {
                    if !undo {
                        state.clear_requirement(*index);
                    }
                    state.restore_requirement(*index, events);
                }
            }
            Self::SinkInsert(key, index, event) => {
                if let Some(state) = states.get_mut(key) {
                    if undo {
                        state.remove_sink_event(*index, *event);
                    } else {
                        state.record_sink(*index, *event);
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
/// Per-rule evidence with a bounded deduplication key set.
///
/// Writes evidence directly into an externally-owned per-rule vec so
/// callers never allocate a second parallel evidence matrix.
pub(super) struct FlowEvidence<'a> {
    /// Evidence grouped by selected rule index, owned by the caller.
    items: &'a mut RuleEvidenceTable,
    /// `(rule, flow, object, event)` identities with emission count per key.
    /// Multiple traces may be emitted for the same key (e.g., different
    /// requirement events from distinct branches).
    emitted: BTreeMap<ReportEvidenceKey, u32>,
    truncated: BTreeSet<ReportEvidenceKey>,
    /// Maximum evidence items emitted (sum of all counts).
    total_emitted: usize,
    /// Whether an emission was rejected by the global limit.
    limit_rejected: bool,
    /// Maximum evidence items emitted for the whole run.
    limit: usize,
    /// Maximum emissions retained for one evidence key.
    max_per_key: u32,
}

/// Maximum number of emissions kept for one evidence key before later traces
/// are truncated.
const MAX_EMISSIONS_PER_KEY: u32 = 256;

impl<'a> FlowEvidence<'a> {
    pub(super) fn new(evidence: &'a mut RuleEvidenceTable, limit: usize) -> Self {
        Self {
            items: evidence,
            emitted: BTreeMap::new(),
            truncated: BTreeSet::new(),
            total_emitted: 0,
            limit_rejected: false,
            limit,
            max_per_key: MAX_EMISSIONS_PER_KEY,
        }
    }

    /// Admit one complete evidence item into the bounded report sink.
    ///
    /// Reservation, catalog insertion, and rollback are one operation, so a
    /// rejected or invalid report index cannot leave the bounded counters out
    /// of sync with the externally owned evidence table.
    pub(super) fn record_if_admitted(
        &mut self,
        key: ReportEvidenceKey,
        rule_index: RuleIndex,
        evidence: ClassificationEvidence,
    ) -> bool {
        if !self.reserve(key) {
            return false;
        }
        if self.items.record(rule_index, evidence).is_err() {
            self.release(key);
            return false;
        }
        true
    }

    fn reserve(&mut self, key: ReportEvidenceKey) -> bool {
        let count = self.emitted.entry(key).or_insert(0);
        if *count >= self.max_per_key {
            self.truncated.insert(key);
            return false;
        }
        if self.total_emitted >= self.limit {
            self.truncated.insert(key);
            self.limit_rejected = true;
            return false;
        }
        *count += 1;
        self.total_emitted += 1;
        true
    }

    fn release(&mut self, key: ReportEvidenceKey) {
        if let Some(count) = self.emitted.get_mut(&key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.emitted.remove(&key);
            }
        }
        self.total_emitted = self.total_emitted.saturating_sub(1);
    }

    #[cfg(test)]
    pub(super) fn emitted_count(&self) -> usize {
        self.total_emitted
    }

    pub(super) fn mark_truncated(&mut self) {
        for key in &self.truncated {
            let _ = self.items.mark_event_truncated(key.rule, key.event.raw());
        }
    }

    pub(super) fn limit_rejected(&self) -> bool {
        self.limit_rejected
    }

    pub(super) fn mark_all_possible(&mut self) {
        self.items.mark_all_possible();
    }
}

#[derive(Debug, Clone)]
/// Saved control construct state used to restore and join environments.
pub(super) enum ControlFrame {
    Branch {
        region: ControlRegionId,
        base: Vec<FlowEnvironment>,
        then_exit: Option<Vec<FlowEnvironment>>,
    },
    Loop {
        region: ControlRegionId,
        body_start: crate::analysis::facts::FactId,
        baseline: Vec<FlowEnvironment>,
        guaranteed: bool,
        breaks: Vec<FlowEnvironment>,
        continues: Vec<FlowEnvironment>,
    },
    Switch {
        region: ControlRegionId,
        baseline: Vec<FlowEnvironment>,
        breaks: Vec<FlowEnvironment>,
        has_default: bool,
    },
    Try {
        region: ControlRegionId,
        baseline: Vec<FlowEnvironment>,
        try_exit: Option<Vec<FlowEnvironment>>,
        catch_exit: Option<Vec<FlowEnvironment>>,
        normal_exit: Option<Vec<FlowEnvironment>>,
        abrupt_exits: Vec<(AbruptExit, FlowEnvironment)>,
        has_finally: bool,
        normal_count: usize,
    },
    Function {
        caller: Vec<FlowEnvironment>,
    },
}

#[derive(Debug, Default)]
pub(super) struct ControlStack {
    frames: Vec<ControlFrame>,
}

/// Loop-frame state taken out of the live stack at the loop end. The frame
/// stays on the stack through the fixed point so the replayed body can keep
/// routing breaks and continues into it.
#[derive(Debug)]
pub(super) struct LoopSeed {
    pub(super) body_start: FactId,
    pub(super) baseline: Vec<FlowEnvironment>,
    pub(super) guaranteed: bool,
    pub(super) breaks: Vec<FlowEnvironment>,
    pub(super) continues: Vec<FlowEnvironment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ControlStackError {
    Empty,
    WrongRegion,
    WrongKind,
    NoTarget,
}

impl ControlStack {
    pub(super) fn push(&mut self, frame: ControlFrame) {
        self.frames.push(frame);
    }

    pub(super) fn last_mut(&mut self) -> Option<&mut ControlFrame> {
        self.frames.last_mut()
    }

    pub(super) fn last_matching_mut(
        &mut self,
        region: ControlRegionId,
    ) -> Result<&mut ControlFrame, ControlStackError> {
        let frame = self.frames.last_mut().ok_or(ControlStackError::Empty)?;
        if frame.region() != Some(region) {
            return Err(ControlStackError::WrongRegion);
        }
        Ok(frame)
    }

    pub(super) fn pop_region(
        &mut self,
        region: ControlRegionId,
    ) -> Result<ControlFrame, ControlStackError> {
        self.last_matching_mut(region)?;
        self.frames.pop().ok_or(ControlStackError::Empty)
    }

    /// Move the fields of the live loop frame that the fixed point no longer
    /// needs out of the stack. The frame itself stays on the stack so the
    /// replayed body can keep routing breaks and continues into it.
    pub(super) fn take_loop_seed(
        &mut self,
        region: ControlRegionId,
    ) -> Result<LoopSeed, ControlStackError> {
        let frame = self.frames.last_mut().ok_or(ControlStackError::Empty)?;
        if frame.region() != Some(region) {
            return Err(ControlStackError::WrongRegion);
        }
        let ControlFrame::Loop {
            body_start,
            baseline,
            guaranteed,
            breaks,
            continues,
            ..
        } = frame
        else {
            return Err(ControlStackError::WrongKind);
        };
        Ok(LoopSeed {
            body_start: *body_start,
            baseline: std::mem::take(baseline),
            guaranteed: *guaranteed,
            breaks: std::mem::take(breaks),
            continues: continues.clone(),
        })
    }

    pub(super) fn pop_loop(&mut self) -> Result<(), ControlStackError> {
        match self.frames.last() {
            Some(ControlFrame::Loop { .. }) => {
                self.frames.pop();
                Ok(())
            }
            Some(_) => Err(ControlStackError::WrongKind),
            None => Err(ControlStackError::Empty),
        }
    }

    pub(super) fn loop_break_count(&self) -> Result<usize, ControlStackError> {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| match frame {
                ControlFrame::Loop { breaks, .. } => Some(breaks.len()),
                _ => None,
            })
            .ok_or(ControlStackError::NoTarget)
    }

    pub(super) fn take_loop_continues(
        &mut self,
    ) -> Result<Vec<FlowEnvironment>, ControlStackError> {
        match self.last_mut() {
            Some(ControlFrame::Loop { continues, .. }) => Ok(std::mem::take(continues)),
            Some(_) => Err(ControlStackError::WrongKind),
            None => Err(ControlStackError::Empty),
        }
    }

    pub(super) fn new_loop_breaks_since(
        &self,
        count: usize,
    ) -> Result<Vec<FlowEnvironment>, ControlStackError> {
        let frame = self.frames.last().ok_or(ControlStackError::Empty)?;
        let ControlFrame::Loop { breaks, .. } = frame else {
            return Err(ControlStackError::WrongKind);
        };
        Ok(breaks.get(count..).unwrap_or_default().to_vec())
    }

    pub(super) fn record_abrupt_exit(&mut self, kind: AbruptExit, environment: &FlowEnvironment) {
        for frame in self.frames.iter_mut().rev() {
            if let ControlFrame::Try { abrupt_exits, .. } = frame {
                abrupt_exits.push((kind, *environment));
            }
        }
    }

    pub(super) fn route_abrupt(
        &mut self,
        kind: AbruptExit,
        environment: FlowEnvironment,
    ) -> Result<(), ControlStackError> {
        match kind {
            AbruptExit::Break => self
                .frames
                .iter_mut()
                .rev()
                .find(|frame| {
                    matches!(
                        frame,
                        ControlFrame::Loop { .. } | ControlFrame::Switch { .. }
                    )
                })
                .map_or(Err(ControlStackError::NoTarget), |frame| {
                    match frame {
                        ControlFrame::Loop { breaks, .. } | ControlFrame::Switch { breaks, .. } => {
                            breaks.push(environment);
                        }
                        _ => unreachable!(),
                    }
                    Ok(())
                }),
            AbruptExit::Continue => {
                if let Some(ControlFrame::Loop { continues, .. }) = self
                    .frames
                    .iter_mut()
                    .rev()
                    .find(|frame| matches!(frame, ControlFrame::Loop { .. }))
                {
                    continues.push(environment);
                    Ok(())
                } else {
                    Err(ControlStackError::NoTarget)
                }
            }
            AbruptExit::Return => Ok(()),
        }
    }

    pub(super) fn pop_function(&mut self) -> Result<Vec<FlowEnvironment>, ControlStackError> {
        match self.frames.last() {
            None => Err(ControlStackError::Empty),
            Some(ControlFrame::Function { .. }) => match self.frames.pop() {
                Some(ControlFrame::Function { caller }) => Ok(caller),
                _ => unreachable!("control stack changed while popping function"),
            },
            Some(_) => Err(ControlStackError::WrongKind),
        }
    }
}

impl ControlFrame {
    fn region(&self) -> Option<ControlRegionId> {
        match self {
            Self::Branch { region, .. }
            | Self::Loop { region, .. }
            | Self::Switch { region, .. }
            | Self::Try { region, .. } => Some(*region),
            Self::Function { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Abrupt completion that must be routed through enclosing control frames.
pub(super) enum AbruptExit {
    /// Exit the nearest loop or switch.
    Break,
    /// Continue the nearest loop.
    Continue,
    /// Exit the current function.
    Return,
}

impl FlowEnvironment {
    pub(super) fn initial() -> Self {
        Self {
            checkpoint: Checkpoint::default(),
            reachable: true,
        }
    }

    /// Whether this snapshot represents a reachable execution path.
    pub(super) fn is_reachable(&self) -> bool {
        self.reachable
    }
}

#[cfg(test)]
mod tests;
