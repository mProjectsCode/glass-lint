//! Control-path state and environment algebra for object-flow projection.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    super::value::{ObjectId, ValueId},
    index::FlowId,
    state::FlowState,
};
use crate::api::classification::ApiEvidence;

type EvidenceKey = (usize, usize, ObjectId, super::super::super::facts::FactId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FlowEnvironment {
    aliases: BTreeMap<ValueId, ObjectId>,
    states: BTreeMap<(ObjectId, FlowId), FlowState>,
    reachable: bool,
}

#[derive(Debug, Default)]
pub(super) struct FlowStateTable {
    aliases: BTreeMap<ValueId, ObjectId>,
    states: BTreeMap<(ObjectId, FlowId), FlowState>,
}
impl FlowStateTable {
    pub(super) fn clear(&mut self) {
        self.aliases.clear();
        self.states.clear();
    }

    pub(super) fn object_for(&self, value: ValueId) -> Option<ObjectId> {
        self.aliases.get(&value).copied()
    }

    pub(super) fn objects(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.aliases.values().copied()
    }

    pub(super) fn bind(&mut self, value: ValueId, object: ObjectId) {
        self.aliases.insert(value, object);
    }

    pub(super) fn unbind(&mut self, value: ValueId) -> Option<ObjectId> {
        self.aliases.remove(&value)
    }

    pub(super) fn has_alias_for(&self, object: ObjectId) -> bool {
        self.aliases.values().any(|alias| *alias == object)
    }

    pub(super) fn states_for(
        &self,
        object: ObjectId,
    ) -> impl Iterator<Item = ((ObjectId, FlowId), &FlowState)> {
        self.states
            .iter()
            .filter(move |((id, _), _)| *id == object)
            .map(|(key, state)| (*key, state))
    }

    pub(super) fn state(&self, object: ObjectId, flow: FlowId) -> Option<&FlowState> {
        self.states.get(&(object, flow))
    }

    pub(super) fn state_mut(&mut self, object: ObjectId, flow: FlowId) -> Option<&mut FlowState> {
        self.states.get_mut(&(object, flow))
    }

    pub(super) fn insert_state(&mut self, state: FlowState) {
        self.states.insert(state.key(), state);
    }

    pub(super) fn state_count(&self) -> usize {
        self.states.len()
    }

    pub(super) fn remove_states_for(&mut self, object: ObjectId) {
        self.states.retain(|(id, _), _| *id != object);
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
pub(super) struct FlowEvidence {
    items: Vec<Vec<ApiEvidence>>,
    emitted: BTreeSet<EvidenceKey>,
}

impl FlowEvidence {
    pub(super) fn new(rule_count: usize) -> Self {
        Self {
            items: vec![Vec::new(); rule_count],
            emitted: BTreeSet::new(),
        }
    }

    pub(super) fn try_insert(&mut self, key: EvidenceKey, limit: usize) -> bool {
        if !self.emitted.contains(&key) && self.emitted.len() >= limit {
            return false;
        }
        self.emitted.insert(key)
    }

    pub(super) fn record(&mut self, rule_index: usize, evidence: ApiEvidence) {
        self.items[rule_index].push(evidence);
    }

    pub(super) fn into_items(self) -> Vec<Vec<ApiEvidence>> {
        self.items
    }
}

#[derive(Debug, Clone)]
pub(super) enum ControlFrame {
    Branch {
        region: u32,
        base: FlowEnvironment,
        then_exit: Option<FlowEnvironment>,
    },
    Loop {
        region: u32,
        baseline: FlowEnvironment,
        guaranteed: bool,
        breaks: Vec<FlowEnvironment>,
        continues: Vec<FlowEnvironment>,
    },
    Switch {
        region: u32,
        baseline: FlowEnvironment,
        breaks: Vec<FlowEnvironment>,
        has_default: bool,
    },
    Try {
        region: u32,
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
pub(super) enum AbruptExit {
    Break,
    Continue,
    Return,
}

impl FlowEnvironment {
    pub(super) fn unreachable() -> Self {
        Self {
            aliases: BTreeMap::new(),
            states: BTreeMap::new(),
            reachable: false,
        }
    }

    pub(super) fn join(left: &Self, right: &Self) -> Self {
        if !left.is_reachable() {
            return right.clone();
        }
        if !right.is_reachable() {
            return left.clone();
        }
        let aliases = left
            .aliases
            .iter()
            .filter_map(|(binding, object)| {
                (right.aliases.get(binding) == Some(object)).then_some((*binding, *object))
            })
            .collect();
        let states = left
            .states
            .iter()
            .filter_map(|(key, left_state)| {
                let right_state = right.states.get(key)?;
                let mut state = left_state.clone();
                state.retain_requirement_keys(right_state);
                Some((*key, state))
            })
            .collect();
        Self {
            aliases,
            states,
            reachable: true,
        }
    }

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

    pub(super) fn is_reachable(&self) -> bool {
        self.reachable
    }
}
