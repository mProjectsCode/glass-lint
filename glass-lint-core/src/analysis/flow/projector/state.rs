//! Control-path state and environment algebra for object-flow projection.
//!
//! Environments are immutable snapshots at branch boundaries. Joining two
//! reachable environments keeps only equal aliases and common requirement
//! keys, which is the precision boundary that prevents path-local facts from
//! leaking after a control-flow merge.

use std::collections::BTreeSet;

use super::super::{
    super::value::{ObjectId, ValueId},
    index::FlowId,
    state::{FlowState, FlowStateKey},
};
use crate::api::classification::ClassificationEvidence;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct ReportEvidenceKey {
    rule: usize,
    flow: usize,
    object: ObjectId,
    event: super::super::super::facts::FactId,
}

impl ReportEvidenceKey {
    pub(super) fn new(
        rule: usize,
        flow: usize,
        object: ObjectId,
        event: super::super::super::facts::FactId,
    ) -> Self {
        Self {
            rule,
            flow,
            object,
            event,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Snapshot of aliases, flow states, and reachability at a control boundary.
pub(super) struct FlowEnvironment {
    /// Value-to-object aliases proven on the snapshot path.
    aliases: AliasTable,
    /// Object/flow lifecycle states proven on the snapshot path.
    states: StateTable,
    /// Whether execution can reach the snapshot.
    reachable: bool,
}

#[derive(Debug, Default)]
/// Mutable live alias and object-state tables for one projector pass.
pub(super) struct FlowStateTable {
    /// Current value aliases, keyed by semantic value identity.
    aliases: AliasTable,
    /// Current lifecycle state for each object and flow matcher.
    states: StateTable,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AliasTable(Vec<(ValueId, ObjectId)>);

impl AliasTable {
    fn position(&self, value: ValueId) -> Result<usize, usize> {
        self.0.binary_search_by_key(&value, |(key, _)| *key)
    }

    fn get(&self, value: ValueId) -> Option<ObjectId> {
        self.position(value).ok().map(|index| self.0[index].1)
    }

    fn insert(&mut self, value: ValueId, object: ObjectId) {
        match self.position(value) {
            Ok(index) => self.0[index].1 = object,
            Err(index) => self.0.insert(index, (value, object)),
        }
    }

    fn remove(&mut self, value: ValueId) -> Option<ObjectId> {
        self.position(value)
            .ok()
            .map(|index| self.0.remove(index).1)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StateTable(Vec<(FlowStateKey, FlowState)>);

impl StateTable {
    fn position(&self, key: FlowStateKey) -> Result<usize, usize> {
        self.0.binary_search_by_key(&key, |(key, _)| *key)
    }

    fn get(&self, key: FlowStateKey) -> Option<&FlowState> {
        self.position(key).ok().map(|index| &self.0[index].1)
    }

    fn get_mut(&mut self, key: FlowStateKey) -> Option<&mut FlowState> {
        self.position(key).ok().map(|index| &mut self.0[index].1)
    }

    fn insert(&mut self, state: FlowState) {
        let key = state.key();
        match self.position(key) {
            Ok(index) => self.0[index].1 = state,
            Err(index) => self.0.insert(index, (key, state)),
        }
    }

    fn remove_object(&mut self, object: ObjectId) {
        self.0.retain(|(key, _)| key.object != object);
    }
}
impl FlowStateTable {
    pub(super) fn clear(&mut self) {
        self.aliases.0.clear();
        self.states.0.clear();
    }

    pub(super) fn object_for(&self, value: ValueId) -> Option<ObjectId> {
        self.aliases.get(value)
    }

    pub(super) fn objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        // Sorted-vector iteration gives callers stable object order for evidence
        // and keeps duplicate aliases from multiplying the same state transition.
        self.aliases.0.iter().map(|(_, object)| *object)
    }

    pub(super) fn bind(&mut self, value: ValueId, object: ObjectId) {
        self.aliases.insert(value, object);
    }

    pub(super) fn unbind(&mut self, value: ValueId) -> Option<ObjectId> {
        self.aliases.remove(value)
    }

    pub(super) fn has_alias_for(&self, object: ObjectId) -> bool {
        self.aliases.0.iter().any(|(_, alias)| *alias == object)
    }

    pub(super) fn states_for(
        &self,
        object: ObjectId,
    ) -> impl Iterator<Item = (FlowStateKey, &FlowState)> {
        self.states
            .0
            .iter()
            .filter(move |(key, _)| key.object == object)
            .map(|(key, state)| (*key, state))
    }

    pub(super) fn state(&self, object: ObjectId, flow: FlowId) -> Option<&FlowState> {
        self.states.get(FlowStateKey { object, flow })
    }

    pub(super) fn state_mut(&mut self, object: ObjectId, flow: FlowId) -> Option<&mut FlowState> {
        self.states.get_mut(FlowStateKey { object, flow })
    }

    pub(super) fn insert_state(&mut self, state: FlowState) {
        self.states.insert(state);
    }

    pub(super) fn state_count(&self) -> usize {
        self.states.0.len()
    }

    pub(super) fn remove_states_for(&mut self, object: ObjectId) {
        self.states.remove_object(object);
    }

    pub(super) fn capture(&self, reachable: bool) -> FlowEnvironment {
        FlowEnvironment {
            aliases: self.aliases.clone(),
            states: self.states.clone(),
            reachable,
        }
    }

    pub(super) fn restore(&mut self, environment: FlowEnvironment) -> bool {
        self.aliases = environment.aliases;
        self.states = environment.states;
        environment.reachable
    }
}

#[derive(Debug)]
/// Per-rule evidence with a bounded deduplication key set.
pub(super) struct FlowEvidence {
    /// Evidence grouped by selected rule index.
    items: Vec<Vec<ClassificationEvidence>>,
    /// `(rule, flow, object, event)` identities already emitted.
    emitted: BTreeSet<ReportEvidenceKey>,
}

impl FlowEvidence {
    pub(super) fn new(rule_count: usize) -> Self {
        Self {
            items: vec![Vec::new(); rule_count],
            emitted: BTreeSet::new(),
        }
    }

    pub(super) fn try_insert(&mut self, key: ReportEvidenceKey, limit: usize) -> bool {
        if !self.emitted.contains(&key) && self.emitted.len() >= limit {
            return false;
        }
        self.emitted.insert(key)
    }

    pub(super) fn record(&mut self, rule_index: usize, evidence: ClassificationEvidence) {
        self.items[rule_index].push(evidence);
    }

    pub(super) fn into_items(self) -> Vec<Vec<ClassificationEvidence>> {
        self.items
    }
}

#[derive(Debug, Clone)]
/// Saved control construct state used to restore and join environments.
pub(super) enum ControlFrame {
    Branch {
        region: super::super::super::facts::ControlRegionId,
        base: FlowEnvironment,
        then_exit: Option<FlowEnvironment>,
    },
    Loop {
        region: super::super::super::facts::ControlRegionId,
        baseline: FlowEnvironment,
        guaranteed: bool,
        breaks: Vec<FlowEnvironment>,
        continues: Vec<FlowEnvironment>,
    },
    Switch {
        region: super::super::super::facts::ControlRegionId,
        baseline: FlowEnvironment,
        breaks: Vec<FlowEnvironment>,
        has_default: bool,
    },
    Try {
        region: super::super::super::facts::ControlRegionId,
        baseline: FlowEnvironment,
        try_exit: Option<FlowEnvironment>,
        catch_exit: Option<FlowEnvironment>,
        normal_exit: Option<FlowEnvironment>,
        abrupt_exits: Vec<(AbruptExit, FlowEnvironment)>,
        has_finally: bool,
    },
    Function {
        caller: FlowEnvironment,
    },
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
    /// Construct an unreachable environment with no usable state.
    pub(super) fn unreachable() -> Self {
        Self {
            aliases: AliasTable::default(),
            states: StateTable::default(),
            reachable: false,
        }
    }

    /// Join two paths, retaining only aliases and requirements proven on both.
    pub(super) fn join(left: &Self, right: &Self) -> Self {
        if !left.is_reachable() {
            return right.clone();
        }
        if !right.is_reachable() {
            return left.clone();
        }
        let aliases = AliasTable(
            left.aliases
                .0
                .iter()
                .filter_map(|(binding, object)| {
                    (right.aliases.get(*binding) == Some(*object)).then_some((*binding, *object))
                })
                .collect(),
        );
        let states = StateTable(
            left.states
                .0
                .iter()
                .filter_map(|(key, left_state)| {
                    let right_state = right.states.get(*key)?;
                    let mut state = left_state.clone();
                    state.retain_requirement_keys(right_state);
                    Some((*key, state))
                })
                .collect(),
        );
        Self {
            aliases,
            states,
            reachable: true,
        }
    }

    /// Join all reachable paths, or return unreachable when none survive.
    pub(super) fn join_many(environments: &[Self]) -> Self {
        let Some(first) = environments
            .iter()
            .find(|environment| environment.is_reachable())
        else {
            return Self::unreachable();
        };
        environments
            .iter()
            .filter(|environment| environment.is_reachable())
            .skip(1)
            .fold(first.clone(), |joined, environment| {
                Self::join(&joined, environment)
            })
    }

    /// Whether this snapshot represents a reachable execution path.
    pub(super) fn is_reachable(&self) -> bool {
        self.reachable
    }
}
