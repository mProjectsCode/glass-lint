//! Control-path state and environment algebra for object-flow projection.
//!
//! Environments are immutable snapshots at branch boundaries. The projector
//! retains a bounded collection of these checkpoints so aliases and lifecycle
//! requirements stay correlated across control-flow merges.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::RangeInclusive,
};

use glass_lint_datastructures::HistoryTransition;

use crate::{
    analysis::{
        facts::{ControlRegionId, FactId},
        flow::projector::history::{Checkpoint, InverseDelta, MutationLog, ReportEvidenceKey},
        model::{
            flow::{FlowId, FlowState, FlowStateKey, RequirementIndex, SinkIndex},
            value::{ObjectId, ValueId},
        },
    },
    api::classification::{ClassificationEvidence, RuleEvidenceTable, RuleIndex},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// O(1) snapshot of the live tables and reachability at a control boundary.
pub(super) struct FlowEnvironment {
    checkpoint: Checkpoint,
    /// Whether execution can reach the snapshot.
    reachable: bool,
}

/// Canonical semantic shape of one live flow environment.
///
/// Object ids are projection-local allocation details.  Loop fixed points
/// must compare the aliases and lifecycle states they identify, not the
/// allocation number assigned during a later replay of the same fact slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalObjectId(u32);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalAlias {
    value: ValueId,
    object: CanonicalObjectId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalRequirementState {
    index: RequirementIndex,
    events: Vec<FactId>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalSinkState {
    index: SinkIndex,
    events: Vec<FactId>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonicalFlowState {
    object: CanonicalObjectId,
    flow: FlowId,
    source_event: FactId,
    requirements: Vec<CanonicalRequirementState>,
    sinks: Vec<CanonicalSinkState>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FlowSemanticSnapshot {
    aliases: Vec<CanonicalAlias>,
    states: Vec<CanonicalFlowState>,
}

#[derive(Debug)]
/// Mutable live alias and object-state tables for one projector pass.
pub(super) struct FlowStateTable {
    /// Current value aliases, keyed by semantic value identity.
    aliases: AliasTable,
    /// Current lifecycle state for each object and flow matcher.
    states: BTreeMap<FlowStateKey, FlowState>,
    /// Mutation log for checkpoint/rollback.
    log: MutationLog,
    /// Maximum number of state entries allowed.
    state_limit: usize,
    state_limit_rejected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Result of admitting one object and its flow-state batch.
pub(super) enum StateAdmission {
    /// Aliases and all batch states were recorded.
    Admitted,
    /// The state limit rejected the batch before any mutation.
    Rejected,
}

/// One plan-selected requirement update for a property-write transition.
#[derive(Debug, Clone, Copy)]
pub(super) struct PropertyWriteUpdate {
    index: RequirementIndex,
    value_matches: bool,
}

impl PropertyWriteUpdate {
    pub(super) fn new(index: RequirementIndex, value_matches: bool) -> Self {
        Self {
            index,
            value_matches,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct AliasTable {
    values: BTreeMap<ValueId, ObjectId>,
    /// Reverse index: how many ValueIds alias each ObjectId.
    object_refs: ObjectRefCounts,
}

impl AliasTable {
    pub(super) fn get(&self, value: ValueId) -> Option<ObjectId> {
        self.values.get(&value).copied()
    }

    fn values(&self) -> impl Iterator<Item = &ObjectId> {
        self.values.values()
    }

    fn iter(&self) -> impl Iterator<Item = (&ValueId, &ObjectId)> {
        self.values.iter()
    }

    pub(super) fn set(&mut self, value: ValueId, object: ObjectId) -> Option<ObjectId> {
        let previous = self.values.insert(value, object);
        if let Some(previous) = previous {
            self.object_refs.decrement(previous);
        }
        self.object_refs.increment(object);
        previous
    }

    pub(super) fn remove(&mut self, value: ValueId) -> Option<ObjectId> {
        let object = self.values.remove(&value)?;
        self.object_refs.decrement(object);
        Some(object)
    }

    fn take(&mut self) -> BTreeMap<ValueId, ObjectId> {
        self.object_refs.clear();
        std::mem::take(&mut self.values)
    }

    fn contains_object(&self, object: ObjectId) -> bool {
        self.object_refs.contains(object)
    }

    fn objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.object_refs.keys()
    }
}

#[derive(Debug, Default)]
struct ObjectRefCounts(BTreeMap<ObjectId, usize>);

impl ObjectRefCounts {
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    pub(super) fn increment(&mut self, object: ObjectId) {
        *self.0.entry(object).or_insert(0) += 1;
    }

    pub(super) fn decrement(&mut self, object: ObjectId) {
        if let Some(count) = self.0.get_mut(&object) {
            *count -= 1;
            if *count == 0 {
                self.0.remove(&object);
            }
        }
    }

    pub(super) fn contains(&self, object: ObjectId) -> bool {
        self.0.contains_key(&object)
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.0.keys().copied()
    }
}

impl FlowStateTable {
    pub(super) fn new(state_limit: usize, mutation_limit: usize) -> Self {
        Self {
            aliases: AliasTable::default(),
            states: BTreeMap::new(),
            log: MutationLog::new(mutation_limit),
            state_limit,
            state_limit_rejected: false,
        }
    }

    pub(super) fn clear(&mut self) {
        let aliases = self.aliases.take();
        for (value, object) in aliases {
            self.log.record(InverseDelta::AliasRemove(value, object));
        }
        let states = std::mem::take(&mut self.states);
        for (key, state) in states {
            self.log
                .record(InverseDelta::StateRemove(key, Box::new(state)));
        }
    }

    pub(super) fn object_for(&self, value: ValueId) -> Option<ObjectId> {
        self.aliases.get(value)
    }

    pub(super) fn object_for_any(&self, values: &[ValueId]) -> Option<ObjectId> {
        values.iter().find_map(|value| self.object_for(*value))
    }

    pub(super) fn objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.aliases.objects()
    }

    fn bind(&mut self, value: ValueId, object: ObjectId) {
        if let Some(old) = self.aliases.set(value, object) {
            self.log
                .record(InverseDelta::AliasUpdate(value, old, object));
        } else {
            self.log.record(InverseDelta::AliasInsert(value, object));
        }
    }

    fn unbind(&mut self, value: ValueId) -> Option<ObjectId> {
        let old_object = self.aliases.remove(value)?;
        self.log
            .record(InverseDelta::AliasRemove(value, old_object));
        Some(old_object)
    }

    fn has_alias_for(&self, object: ObjectId) -> bool {
        self.aliases.contains_object(object)
    }

    pub(super) fn bind_aliases(&mut self, values: &[ValueId], object: ObjectId) {
        for value in values {
            self.bind(*value, object);
        }
    }

    pub(super) fn unbind_aliases(&mut self, values: &[ValueId]) {
        let mut objects = BTreeSet::new();
        for value in values {
            if let Some(object) = self.unbind(*value) {
                objects.insert(object);
            }
        }
        for object in objects {
            if !self.has_alias_for(object) {
                self.remove_states_for(object);
            }
        }
    }

    pub(super) fn invalidate_aliases(&mut self, values: &[ValueId]) {
        let objects = values
            .iter()
            .filter_map(|value| self.object_for(*value))
            .collect::<BTreeSet<_>>();
        for object in objects {
            self.remove_states_for(object);
        }
    }

    fn object_range(object: ObjectId) -> RangeInclusive<FlowStateKey> {
        FlowStateKey::new(object, FlowId::new(RuleIndex::new(0), 0))
            ..=FlowStateKey::new(object, FlowId::new(RuleIndex::new(usize::MAX), usize::MAX))
    }

    pub(super) fn states_for(
        &self,
        object: ObjectId,
    ) -> impl Iterator<Item = (FlowStateKey, &FlowState)> + '_ {
        self.states
            .range(Self::object_range(object))
            .map(|(key, state)| (*key, state))
    }

    pub(super) fn state(&self, object: ObjectId, flow: FlowId) -> Option<&FlowState> {
        let key = FlowStateKey::new(object, flow);
        self.states.get(&key)
    }

    pub(super) fn record_requirement(
        &mut self,
        object: ObjectId,
        flow: FlowId,
        index: RequirementIndex,
        event: crate::analysis::facts::FactId,
    ) -> bool {
        let key = FlowStateKey::new(object, flow);
        let Some(state) = self.states.get_mut(&key) else {
            return false;
        };
        if state.record_requirement(index, event) {
            self.log
                .record(InverseDelta::RequirementInsert(key, index, event));
            true
        } else {
            false
        }
    }

    pub(super) fn clear_requirement(
        &mut self,
        object: ObjectId,
        flow: FlowId,
        index: RequirementIndex,
    ) -> bool {
        let key = FlowStateKey::new(object, flow);
        let Some(state) = self.states.get_mut(&key) else {
            return false;
        };
        let Some(events) = state.clear_requirement(index) else {
            return false;
        };
        self.log
            .record(InverseDelta::RequirementRemove(key, index, events));
        true
    }

    /// Apply all plan-selected property-write updates for one live object.
    ///
    /// The caller supplies only plan-specific matching results; state-key
    /// traversal and the reversible clear/record mutation protocol remain
    /// owned by this table.
    pub(super) fn apply_property_write(
        &mut self,
        object: ObjectId,
        event: FactId,
        mut updates_for: impl FnMut(FlowId) -> Vec<PropertyWriteUpdate>,
    ) -> Vec<FlowId> {
        let flows = self
            .states_for(object)
            .map(|(key, _)| key.flow())
            .collect::<Vec<_>>();
        let mut affected_flows = Vec::new();
        for flow in flows {
            let requirement_updates = updates_for(flow);
            if requirement_updates.is_empty() {
                continue;
            }
            for update in requirement_updates {
                self.clear_requirement(object, flow, update.index);
                if update.value_matches {
                    self.record_requirement(object, flow, update.index, event);
                }
            }
            affected_flows.push(flow);
        }
        affected_flows
    }

    pub(super) fn record_sink(
        &mut self,
        object: ObjectId,
        flow: FlowId,
        index: SinkIndex,
        event: crate::analysis::facts::FactId,
    ) -> bool {
        let key = FlowStateKey::new(object, flow);
        let Some(state) = self.states.get_mut(&key) else {
            return false;
        };
        if state.record_sink(index, event) {
            self.log.record(InverseDelta::SinkInsert(key, index, event));
            true
        } else {
            false
        }
    }

    /// Insert or update one state. Returns `false` when the state limit has
    /// been reached and the insertion was rejected.
    #[cfg(test)]
    pub(super) fn insert_state(&mut self, state: FlowState) -> bool {
        let key = state.key();
        if !self.states.contains_key(&key) && self.states.len() >= self.state_limit {
            self.state_limit_rejected = true;
            false
        } else {
            self.insert_state_unchecked(state);
            true
        }
    }

    fn insert_state_unchecked(&mut self, state: FlowState) {
        let key = state.key();
        if let Some(old) = self.states.insert(key, state.clone()) {
            self.log.record(InverseDelta::StateUpdate(
                key,
                Box::new(old),
                Box::new(state),
            ));
        } else {
            self.log
                .record(InverseDelta::StateInsert(key, Box::new(state)));
        }
    }

    /// Admit one object alias and its states as an atomic capacity decision.
    ///
    /// Existing state keys are updates and do not consume capacity. A
    /// rejected batch leaves aliases and states unchanged while recording the
    /// fail-closed state-limit outcome.
    pub(super) fn admit_object(
        &mut self,
        aliases: &[ValueId],
        object: ObjectId,
        states: &[FlowState],
    ) -> StateAdmission {
        let mut new_keys = BTreeSet::new();
        for state in states {
            let key = state.key();
            if !self.states.contains_key(&key) {
                new_keys.insert(key);
            }
        }
        if self.states.len().saturating_add(new_keys.len()) > self.state_limit {
            self.state_limit_rejected = true;
            return StateAdmission::Rejected;
        }
        self.bind_aliases(aliases, object);
        for state in states {
            self.insert_state_unchecked(state.clone());
        }
        StateAdmission::Admitted
    }

    #[cfg(test)]
    pub(super) fn state_count(&self) -> usize {
        self.states.len()
    }

    /// Return a deterministic semantic snapshot with object ids normalized to
    /// their first appearance in the alias table.  This intentionally omits
    /// checkpoints and allocation counters so repeated loop iterations can
    /// converge even when they allocate fresh projection-local objects.
    pub(super) fn semantic_snapshot(&self) -> FlowSemanticSnapshot {
        let mut objects = BTreeMap::new();
        let mut next = 0u32;
        for object in self.aliases.values() {
            objects.entry(*object).or_insert_with(|| {
                let id = CanonicalObjectId(next);
                next = next.saturating_add(1);
                id
            });
        }
        let aliases = self
            .aliases
            .iter()
            .filter_map(|(value, object)| {
                objects.get(object).copied().map(|object| CanonicalAlias {
                    value: *value,
                    object,
                })
            })
            .collect();
        let states = self
            .states
            .iter()
            .filter_map(|(key, state)| {
                // A state without a live alias cannot reach a later transfer.
                // Do not let stale, unreachable state from an overwritten
                // loop binding keep changing the fixed-point shape.
                let object = objects.get(&key.object()).copied()?;
                let requirements = state
                    .requirement_entries()
                    .map(|(index, values)| CanonicalRequirementState {
                        index,
                        events: values,
                    })
                    .collect();
                let sinks = state
                    .sink_entries()
                    .map(|(index, values)| CanonicalSinkState {
                        index,
                        events: values,
                    })
                    .collect();
                Some(CanonicalFlowState {
                    object,
                    flow: key.flow(),
                    source_event: state.source_event(),
                    requirements,
                    sinks,
                })
            })
            .collect();
        FlowSemanticSnapshot { aliases, states }
    }

    #[cfg(test)]
    pub(super) fn mutation_count(&self) -> usize {
        self.log.node_count()
    }

    pub(super) fn mutation_exhausted(&self) -> bool {
        self.log.is_budget_exhausted()
    }

    pub(super) fn state_limit_rejected(&self) -> bool {
        self.state_limit_rejected
    }

    fn remove_states_for(&mut self, object: ObjectId) {
        let keys: Vec<FlowStateKey> = self
            .states
            .range(Self::object_range(object))
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            if let Some(state) = self.states.remove(&key) {
                self.log
                    .record(InverseDelta::StateRemove(key, Box::new(state)));
            }
        }
    }

    /// Record a checkpoint at the current mutation log position.
    pub(super) fn capture(&self, reachable: bool) -> FlowEnvironment {
        FlowEnvironment {
            checkpoint: self.log.checkpoint(),
            reachable,
        }
    }

    /// Restore a previously captured environment by rolling back the mutation
    /// log to the checkpoint that corresponds to the environment.
    pub(super) fn restore(&mut self, environment: FlowEnvironment) -> bool {
        let Self {
            aliases,
            states,
            log,
            ..
        } = self;
        if log.transition(environment.checkpoint, |direction, delta| match direction {
            HistoryTransition::Undo => Self::apply_inverse(delta, aliases, states),
            HistoryTransition::Redo => Self::apply_forward(delta, aliases, states),
        }) {
            environment.reachable
        } else {
            false
        }
    }

    fn apply_inverse(
        delta: &InverseDelta,
        aliases: &mut AliasTable,
        states: &mut BTreeMap<FlowStateKey, FlowState>,
    ) {
        match delta {
            InverseDelta::AliasInsert(value, _) => {
                aliases.remove(*value);
            }
            InverseDelta::AliasUpdate(value, old, _) => {
                aliases.set(*value, *old);
            }
            InverseDelta::AliasRemove(value, object) => {
                aliases.set(*value, *object);
            }
            InverseDelta::StateInsert(key, _) => {
                states.remove(key);
            }
            InverseDelta::StateUpdate(key, old, _) => {
                states.insert(*key, (**old).clone());
            }
            InverseDelta::StateRemove(key, state) => {
                states.insert(*key, (**state).clone());
            }
            InverseDelta::RequirementInsert(key, index, event) => {
                if let Some(state) = states.get_mut(key) {
                    state.remove_requirement_event(*index, *event);
                }
            }
            InverseDelta::RequirementRemove(key, index, events) => {
                if let Some(state) = states.get_mut(key) {
                    state.restore_requirement(*index, events);
                }
            }
            InverseDelta::SinkInsert(key, index, event) => {
                if let Some(state) = states.get_mut(key) {
                    state.remove_sink_event(*index, *event);
                }
            }
        }
    }

    fn apply_forward(
        delta: &InverseDelta,
        aliases: &mut AliasTable,
        states: &mut BTreeMap<FlowStateKey, FlowState>,
    ) {
        match delta {
            InverseDelta::AliasInsert(value, object) => {
                aliases.set(*value, *object);
            }
            InverseDelta::AliasUpdate(value, _old, new) => {
                aliases.set(*value, *new);
            }
            InverseDelta::AliasRemove(value, _object) => {
                aliases.remove(*value);
            }
            InverseDelta::StateInsert(key, state) => {
                states.insert(*key, (**state).clone());
            }
            InverseDelta::StateUpdate(key, _, new) => {
                states.insert(*key, (**new).clone());
            }
            InverseDelta::StateRemove(key, _) => {
                states.remove(key);
            }
            InverseDelta::RequirementInsert(key, index, event) => {
                if let Some(state) = states.get_mut(key) {
                    state.record_requirement(*index, *event);
                }
            }
            InverseDelta::RequirementRemove(key, index, events) => {
                if let Some(state) = states.get_mut(key) {
                    state.clear_requirement(*index);
                    state.restore_requirement(*index, events);
                }
            }
            InverseDelta::SinkInsert(key, index, event) => {
                if let Some(state) = states.get_mut(key) {
                    state.record_sink(*index, *event);
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
}

impl<'a> FlowEvidence<'a> {
    pub(super) fn new(evidence: &'a mut RuleEvidenceTable) -> Self {
        Self {
            items: evidence,
            emitted: BTreeMap::new(),
            truncated: BTreeSet::new(),
            total_emitted: 0,
            limit_rejected: false,
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
        limit: usize,
        max_per_key: u32,
        rule_index: RuleIndex,
        evidence: ClassificationEvidence,
    ) -> bool {
        if !self.reserve(key, limit, max_per_key) {
            return false;
        }
        if self.items.record(rule_index, evidence).is_err() {
            self.release(key);
            return false;
        }
        true
    }

    fn reserve(&mut self, key: ReportEvidenceKey, limit: usize, max_per_key: u32) -> bool {
        let count = self.emitted.entry(key).or_insert(0);
        if *count >= max_per_key {
            self.truncated.insert(key);
            return false;
        }
        if self.total_emitted >= limit {
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

    pub(super) fn loop_frame(
        &self,
        region: ControlRegionId,
    ) -> Result<ControlFrame, ControlStackError> {
        let frame = self.frames.last().ok_or(ControlStackError::Empty)?;
        if frame.region() != Some(region) {
            return Err(ControlStackError::WrongRegion);
        }
        match frame {
            ControlFrame::Loop { .. } => Ok(frame.clone()),
            _ => Err(ControlStackError::WrongKind),
        }
    }

    pub(super) fn pop_loop(&mut self, body_start: FactId) -> Result<(), ControlStackError> {
        let frame = self.frames.last().ok_or(ControlStackError::Empty)?;
        match frame {
            ControlFrame::Loop {
                body_start: expected,
                ..
            } if *expected == body_start => {
                self.frames.pop();
                Ok(())
            }
            ControlFrame::Loop { .. } => Err(ControlStackError::WrongRegion),
            _ => Err(ControlStackError::WrongKind),
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
mod tests {
    use super::*;
    use crate::{
        analysis::model::fact::{ControlRegionId, FactId},
        api::classification::RuleIndex,
    };

    #[test]
    fn mismatched_region_pop_preserves_the_top_frame() {
        let mut stack = ControlStack::default();
        stack.push(ControlFrame::Branch {
            region: ControlRegionId::from_test(1),
            base: vec![FlowEnvironment::initial()],
            then_exit: None,
        });

        assert!(matches!(
            stack.pop_region(ControlRegionId::from_test(2)),
            Err(ControlStackError::WrongRegion)
        ));
        assert!(matches!(
            stack.last_mut(),
            Some(ControlFrame::Branch { .. })
        ));
        assert!(stack.pop_region(ControlRegionId::from_test(1)).is_ok());
    }

    #[test]
    fn wrong_function_exit_preserves_the_top_frame() {
        let mut stack = ControlStack::default();
        stack.push(ControlFrame::Branch {
            region: ControlRegionId::from_test(1),
            base: vec![FlowEnvironment::initial()],
            then_exit: None,
        });

        assert_eq!(stack.pop_function(), Err(ControlStackError::WrongKind));
        assert!(matches!(
            stack.last_mut(),
            Some(ControlFrame::Branch { .. })
        ));
    }

    #[test]
    fn empty_loop_operations_report_missing_frames() {
        let mut stack = ControlStack::default();
        assert_eq!(stack.take_loop_continues(), Err(ControlStackError::Empty));
        assert_eq!(
            stack.new_loop_breaks_since(0),
            Err(ControlStackError::Empty)
        );
    }

    fn test_evidence() -> ClassificationEvidence {
        ClassificationEvidence::from_occurrences(
            crate::api::classification::MatchKind::CallArgument,
            "test".to_owned(),
            vec![
                crate::api::classification::ClassificationEvidenceOccurrence::new(
                    glass_lint_datastructures::ByteRange::empty(),
                    Some(1),
                    None,
                ),
            ],
            crate::project::MatchCertainty::Definite,
        )
        .expect("test evidence has one occurrence")
    }

    #[test]
    fn checkpoints_restore_divergent_mutation_paths() {
        let mut table = FlowStateTable::new(262_144, 4096);
        table.bind(ValueId::from_test(1), ObjectId::from_test(1));
        let base = table.capture(true);

        table.bind(ValueId::from_test(2), ObjectId::from_test(2));
        let left = table.capture(true);
        assert!(table.restore(base));
        assert_eq!(table.object_for(ValueId::from_test(2)), None);

        table.bind(ValueId::from_test(3), ObjectId::from_test(3));
        assert!(table.restore(left));
        assert_eq!(
            table.object_for(ValueId::from_test(2)),
            Some(ObjectId::from_test(2))
        );
        assert_eq!(table.object_for(ValueId::from_test(3)), None);
        assert!(table.restore(base));
        assert_eq!(
            table.object_for(ValueId::from_test(1)),
            Some(ObjectId::from_test(1))
        );
    }

    #[test]
    fn bind_updates_and_unbind_removes_aliases() {
        let mut table = FlowStateTable::new(100, 100);
        table.bind(ValueId::from_test(1), ObjectId::from_test(10));
        assert_eq!(
            table.object_for(ValueId::from_test(1)),
            Some(ObjectId::from_test(10))
        );
        assert!(table.has_alias_for(ObjectId::from_test(10)));

        table.bind(ValueId::from_test(1), ObjectId::from_test(20));
        assert_eq!(
            table.object_for(ValueId::from_test(1)),
            Some(ObjectId::from_test(20))
        );

        let removed = table.unbind(ValueId::from_test(1));
        assert_eq!(removed, Some(ObjectId::from_test(20)));
        assert_eq!(table.object_for(ValueId::from_test(1)), None);
        assert!(!table.has_alias_for(ObjectId::from_test(20)));
    }

    #[test]
    fn object_for_returns_none_for_unbound_value() {
        let table = FlowStateTable::new(100, 100);
        assert_eq!(table.object_for(ValueId::from_test(99)), None);
    }

    #[test]
    fn has_alias_for_false_when_no_aliases_exist() {
        let table = FlowStateTable::new(100, 100);
        assert!(!table.has_alias_for(ObjectId::from_test(1)));
    }

    #[test]
    fn objects_are_unique_for_multiple_aliases() {
        let mut table = FlowStateTable::new(100, 100);
        table.bind(ValueId::from_test(1), ObjectId::from_test(1));
        table.bind(ValueId::from_test(2), ObjectId::from_test(1));
        table.bind(ValueId::from_test(3), ObjectId::from_test(2));

        assert_eq!(
            table.objects().collect::<Vec<_>>(),
            vec![ObjectId::from_test(1), ObjectId::from_test(2)]
        );
    }

    #[test]
    fn unbind_aliases_cleans_state_only_after_the_last_alias() {
        let mut table = FlowStateTable::new(100, 100);
        let aliases = [ValueId::from_test(1), ValueId::from_test(2)];
        let object = ObjectId::from_test(1);
        table.bind_aliases(&aliases, object);
        table.insert_state(FlowState::new(
            FlowId::new(RuleIndex::new(0), 0),
            FactId::from_test(1),
            object,
        ));

        table.unbind_aliases(&aliases[..1]);
        assert_eq!(table.states_for(object).count(), 1);
        table.unbind_aliases(&aliases[1..]);
        assert_eq!(table.states_for(object).count(), 0);
    }

    #[test]
    fn state_limit_rejects_insertion_beyond_capacity() {
        let mut table = FlowStateTable::new(2, 100);
        let state1 = FlowState::new(
            FlowId::new(RuleIndex::new(0), 0),
            FactId::from_test(1),
            ObjectId::from_test(1),
        );
        let state2 = FlowState::new(
            FlowId::new(RuleIndex::new(0), 1),
            FactId::from_test(2),
            ObjectId::from_test(2),
        );
        let state3 = FlowState::new(
            FlowId::new(RuleIndex::new(0), 2),
            FactId::from_test(3),
            ObjectId::from_test(3),
        );
        assert!(table.insert_state(state1));
        assert!(table.insert_state(state2));
        assert!(!table.insert_state(state3));
        assert_eq!(table.state_count(), 2);
    }

    #[test]
    fn admit_object_counts_updates_without_rejecting_the_batch() {
        let mut table = FlowStateTable::new(2, 100);
        let existing = FlowState::new(
            FlowId::new(RuleIndex::new(0), 0),
            FactId::from_test(1),
            ObjectId::from_test(1),
        );
        table.insert_state(existing);
        let update = FlowState::new(
            FlowId::new(RuleIndex::new(0), 0),
            FactId::from_test(2),
            ObjectId::from_test(1),
        );
        let new_state = FlowState::new(
            FlowId::new(RuleIndex::new(0), 1),
            FactId::from_test(3),
            ObjectId::from_test(2),
        );

        assert_eq!(
            table.admit_object(
                &[ValueId::from_test(2)],
                ObjectId::from_test(2),
                &[update, new_state]
            ),
            StateAdmission::Admitted
        );
        assert_eq!(
            table.object_for(ValueId::from_test(2)),
            Some(ObjectId::from_test(2))
        );
        assert_eq!(table.state_count(), 2);
        assert_eq!(
            table
                .state(ObjectId::from_test(1), FlowId::new(RuleIndex::new(0), 0))
                .map(FlowState::source_event),
            Some(FactId::from_test(2))
        );
    }

    #[test]
    fn rejected_object_admission_does_not_bind_or_insert() {
        let mut table = FlowStateTable::new(1, 100);
        let existing = FlowState::new(
            FlowId::new(RuleIndex::new(0), 0),
            FactId::from_test(1),
            ObjectId::from_test(1),
        );
        table.insert_state(existing);
        let rejected = FlowState::new(
            FlowId::new(RuleIndex::new(0), 1),
            FactId::from_test(2),
            ObjectId::from_test(2),
        );

        assert_eq!(
            table.admit_object(
                &[ValueId::from_test(2)],
                ObjectId::from_test(2),
                &[rejected]
            ),
            StateAdmission::Rejected
        );
        assert_eq!(table.object_for(ValueId::from_test(2)), None);
        assert_eq!(table.state_count(), 1);
        assert!(table.state_limit_rejected());
    }

    #[test]
    fn remove_states_for_clears_all_object_states() {
        let mut table = FlowStateTable::new(100, 100);
        table.bind(ValueId::from_test(1), ObjectId::from_test(1));
        table.bind(ValueId::from_test(2), ObjectId::from_test(1));
        let s1 = FlowState::new(
            FlowId::new(RuleIndex::new(0), 0),
            FactId::from_test(1),
            ObjectId::from_test(1),
        );
        let s2 = FlowState::new(
            FlowId::new(RuleIndex::new(0), 1),
            FactId::from_test(2),
            ObjectId::from_test(2),
        );
        table.insert_state(s1);
        table.insert_state(s2);
        table.remove_states_for(ObjectId::from_test(1));
        assert_eq!(table.states_for(ObjectId::from_test(1)).count(), 0);
        assert_eq!(table.state_count(), 1);
    }

    #[test]
    fn mutation_count_tracks_mutations() {
        let mut table = FlowStateTable::new(100, 100);
        assert_eq!(table.mutation_count(), 0);
        table.bind(ValueId::from_test(1), ObjectId::from_test(10));
        assert_eq!(table.mutation_count(), 1);
        table.bind(ValueId::from_test(2), ObjectId::from_test(20));
        assert_eq!(table.mutation_count(), 2);
        table.unbind(ValueId::from_test(1));
        assert_eq!(table.mutation_count(), 3);
    }

    #[test]
    fn clear_removes_all_aliases_and_states() {
        let mut table = FlowStateTable::new(100, 100);
        table.bind(ValueId::from_test(1), ObjectId::from_test(10));
        table.bind(ValueId::from_test(2), ObjectId::from_test(20));
        let s = FlowState::new(
            FlowId::new(RuleIndex::new(0), 0),
            FactId::from_test(1),
            ObjectId::from_test(10),
        );
        table.insert_state(s);
        table.clear();
        assert_eq!(table.object_for(ValueId::from_test(1)), None);
        assert_eq!(table.object_for(ValueId::from_test(2)), None);
        assert_eq!(table.state_count(), 0);
    }

    #[test]
    fn distinct_semantic_snapshots_remain_distinct() {
        let mut table = FlowStateTable::new(100, 100);
        table.bind(ValueId::from_test(1), ObjectId::from_test(1));
        let first = table.semantic_snapshot();

        table.bind(ValueId::from_test(2), ObjectId::from_test(2));
        let second = table.semantic_snapshot();

        assert_ne!(first, second);
    }

    #[test]
    fn evidence_limit_rejects_repeated_emissions_for_existing_key() {
        let mut items = RuleEvidenceTable::new_for_test(1);
        let mut evidence = FlowEvidence::new(&mut items);
        let key = ReportEvidenceKey::new(
            RuleIndex::new(0),
            0,
            ObjectId::from_test(1),
            FactId::from_test(1),
        );

        assert!(evidence.record_if_admitted(key, 1, 256, RuleIndex::new(0), test_evidence(),));
        assert!(!evidence.record_if_admitted(key, 1, 256, RuleIndex::new(0), test_evidence(),));
        assert_eq!(evidence.emitted_count(), 1);
        assert!(evidence.limit_rejected());
    }

    #[test]
    fn evidence_limit_rejects_new_keys_after_capacity_is_full() {
        let mut items = RuleEvidenceTable::new_for_test(1);
        let mut evidence = FlowEvidence::new(&mut items);
        let first = ReportEvidenceKey::new(
            RuleIndex::new(0),
            0,
            ObjectId::from_test(1),
            FactId::from_test(1),
        );
        let second = ReportEvidenceKey::new(
            RuleIndex::new(0),
            0,
            ObjectId::from_test(2),
            FactId::from_test(2),
        );

        assert!(evidence.record_if_admitted(first, 2, 256, RuleIndex::new(0), test_evidence(),));
        assert!(evidence.record_if_admitted(second, 2, 256, RuleIndex::new(0), test_evidence(),));
        assert!(!evidence.record_if_admitted(first, 2, 256, RuleIndex::new(0), test_evidence(),));
        assert!(!evidence.record_if_admitted(second, 2, 256, RuleIndex::new(0), test_evidence(),));
        assert_eq!(evidence.emitted_count(), 2);
        assert!(evidence.limit_rejected());
    }

    #[test]
    fn fine_grained_state_edits_restore_across_checkpoints() {
        let mut table = FlowStateTable::new(100, 100);
        let flow = FlowId::new(RuleIndex::new(0), 0);
        let state = FlowState::new(flow, FactId::from_test(1), ObjectId::from_test(10));
        table.insert_state(state);
        let base = table.capture(true);
        assert!(table.record_requirement(
            ObjectId::from_test(10),
            flow,
            RequirementIndex::new(0).unwrap(),
            FactId::from_test(5),
        ));
        assert!(table.record_requirement(
            ObjectId::from_test(10),
            flow,
            RequirementIndex::new(0).unwrap(),
            FactId::from_test(7),
        ));
        assert!(table.record_sink(
            ObjectId::from_test(10),
            flow,
            SinkIndex::new(0).unwrap(),
            FactId::from_test(6),
        ));
        let retrieved = table.state(ObjectId::from_test(10), flow).unwrap();
        assert_eq!(retrieved.source_event(), FactId::from_test(1));
        assert_eq!(retrieved.requirement_entries().count(), 1);
        assert_eq!(retrieved.sink_entries().count(), 1);

        let configured = table.capture(true);
        assert!(table.clear_requirement(
            ObjectId::from_test(10),
            flow,
            RequirementIndex::new(0).unwrap(),
        ));
        assert_eq!(
            table
                .state(ObjectId::from_test(10), flow)
                .unwrap()
                .requirement_entries()
                .count(),
            0
        );
        assert!(table.restore(configured));
        let restored = table.state(ObjectId::from_test(10), flow).unwrap();
        assert_eq!(restored.requirement_entries().next().unwrap().1.len(), 2);

        assert!(table.restore(base));
        let restored = table.state(ObjectId::from_test(10), flow).unwrap();
        assert_eq!(restored.requirement_entries().count(), 0);
        assert_eq!(restored.sink_entries().count(), 0);

        assert!(table.record_requirement(
            ObjectId::from_test(10),
            flow,
            RequirementIndex::new(1).unwrap(),
            FactId::from_test(7),
        ));
        assert!(table.restore(base));
        assert_eq!(
            table
                .state(ObjectId::from_test(10), flow)
                .unwrap()
                .requirement_entries()
                .count(),
            0
        );
    }
}
