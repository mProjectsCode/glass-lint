use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::{
    facts::FactId,
    model::flow::{FlowState, FlowStateKey},
    value::{ObjectId, ValueId},
};

/// An inverse delta that can undo one mutation on an alias or state table.
#[derive(Debug, Clone)]
pub(super) enum InverseDelta {
    AliasInsert(ValueId, ObjectId),
    AliasUpdate(ValueId, ObjectId, ObjectId),
    AliasRemove(ValueId, ObjectId),
    StateInsert(FlowStateKey, Box<FlowState>),
    StateUpdate(FlowStateKey, Box<FlowState>, Box<FlowState>),
    StateRemove(FlowStateKey, Box<FlowState>),
    RequirementInsert(FlowStateKey, usize, FactId),
    RequirementRemove(FlowStateKey, usize, BTreeSet<FactId>),
    SinkInsert(FlowStateKey, usize, FactId),
}

/// A position in the persistent mutation history that acts as a checkpoint.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Checkpoint(pub(super) usize);

#[derive(Debug)]
struct LogNode {
    parent: usize,
    depth: usize,
    delta: InverseDelta,
}

/// A bounded parent-linked mutation history. Checkpoints are O(1); moving
/// between them applies only the deltas on the paths between the checkpoints.
#[derive(Debug)]
pub(super) struct MutationLog {
    nodes: Vec<LogNode>,
    cursor: usize,
    budget_exhausted: bool,
    limit: usize,
    /// Total charge count including mutation records and comparison charges.
    /// Used to bound CPU work from join comparisons against the same budget
    /// that bounds mutation-log output.
    charges: usize,
}

impl MutationLog {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            nodes: Vec::new(),
            cursor: 0,
            budget_exhausted: false,
            limit,
            charges: 0,
        }
    }

    #[allow(dead_code)]
    pub(super) fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub(super) fn is_budget_exhausted(&self) -> bool {
        self.budget_exhausted
    }

    pub(super) fn record(&mut self, delta: InverseDelta) {
        if self.charges >= self.limit {
            self.budget_exhausted = true;
            return;
        }
        self.charges += 1;
        let parent = self.cursor;
        let depth = self.depth(parent) + 1;
        self.nodes.push(LogNode {
            parent,
            depth,
            delta,
        });
        self.cursor = self.nodes.len();
    }

    pub(super) fn checkpoint(&self) -> Checkpoint {
        Checkpoint(self.cursor)
    }

    pub(super) fn transition(
        &mut self,
        checkpoint: Checkpoint,
        aliases: &mut BTreeMap<ValueId, ObjectId>,
        object_refs: &mut BTreeMap<ObjectId, usize>,
        states: &mut BTreeMap<FlowStateKey, FlowState>,
    ) -> bool {
        if checkpoint.0 > self.nodes.len() || self.budget_exhausted {
            return false;
        }
        let mut current = self.cursor;
        let mut target = checkpoint.0;
        while self.depth(current) > self.depth(target) {
            current = self.nodes[current - 1].parent;
        }
        while self.depth(target) > self.depth(current) {
            target = self.nodes[target - 1].parent;
        }
        while current != target {
            current = self.nodes[current - 1].parent;
            target = self.nodes[target - 1].parent;
        }
        let lca = current;
        let mut node = self.cursor;
        while node != lca {
            apply_inverse(&self.nodes[node - 1].delta, aliases, object_refs, states);
            node = self.nodes[node - 1].parent;
        }
        let mut forward = Vec::new();
        node = checkpoint.0;
        while node != lca {
            forward.push(node);
            node = self.nodes[node - 1].parent;
        }
        for node in forward.into_iter().rev() {
            apply_forward(&self.nodes[node - 1].delta, aliases, object_refs, states);
        }
        self.cursor = checkpoint.0;
        true
    }

    fn depth(&self, node: usize) -> usize {
        if node == 0 {
            return 0;
        }
        self.nodes
            .get(node.saturating_sub(1))
            .map_or(0, |entry| entry.depth)
    }
}

pub(super) fn increment_ref(refs: &mut BTreeMap<ObjectId, usize>, object: ObjectId) {
    *refs.entry(object).or_insert(0) += 1;
}

pub(super) fn decrement_ref(refs: &mut BTreeMap<ObjectId, usize>, object: ObjectId) {
    if let Some(count) = refs.get_mut(&object) {
        *count -= 1;
        if *count == 0 {
            refs.remove(&object);
        }
    }
}

fn apply_inverse(
    delta: &InverseDelta,
    aliases: &mut BTreeMap<ValueId, ObjectId>,
    object_refs: &mut BTreeMap<ObjectId, usize>,
    states: &mut BTreeMap<FlowStateKey, FlowState>,
) {
    match delta {
        InverseDelta::AliasInsert(value, _) => {
            if let Some(object) = aliases.remove(value) {
                decrement_ref(object_refs, object);
            }
        }
        InverseDelta::AliasUpdate(value, old, _) => {
            if let Some(prev) = aliases.insert(*value, *old) {
                decrement_ref(object_refs, prev);
                increment_ref(object_refs, *old);
            }
        }
        InverseDelta::AliasRemove(value, object) => {
            aliases.insert(*value, *object);
            increment_ref(object_refs, *object);
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
    aliases: &mut BTreeMap<ValueId, ObjectId>,
    object_refs: &mut BTreeMap<ObjectId, usize>,
    states: &mut BTreeMap<FlowStateKey, FlowState>,
) {
    match delta {
        InverseDelta::AliasInsert(value, object) => {
            aliases.insert(*value, *object);
            increment_ref(object_refs, *object);
        }
        InverseDelta::AliasUpdate(value, old, new) => {
            aliases.insert(*value, *new);
            decrement_ref(object_refs, *old);
            increment_ref(object_refs, *new);
        }
        InverseDelta::AliasRemove(value, object) => {
            aliases.remove(value);
            decrement_ref(object_refs, *object);
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
                for event in events {
                    state.record_requirement(*index, *event);
                }
            }
        }
        InverseDelta::SinkInsert(key, index, event) => {
            if let Some(state) = states.get_mut(key) {
                state.record_sink(*index, *event);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct ReportEvidenceKey {
    pub(super) rule: usize,
    pub(super) flow: usize,
    pub(super) object: ObjectId,
    pub(super) event: FactId,
}

impl ReportEvidenceKey {
    pub(super) fn new(rule: usize, flow: usize, object: ObjectId, event: FactId) -> Self {
        Self {
            rule,
            flow,
            object,
            event,
        }
    }
}
