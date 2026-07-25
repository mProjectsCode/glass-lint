use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

use crate::{
    analysis::model::fact::FactId,
    api::{classification::RuleIndex, compiler::CompiledObjectFlow},
};

#[derive(Debug, Clone, Copy)]
pub struct FlowLimits {
    objects: u32,
    states: usize,
    emissions: usize,
    mutation: usize,
}

const DEFAULT_OBJECTS: u64 = 65_536;
const DEFAULT_STATES: u64 = 262_144;
const DEFAULT_EMISSIONS: u64 = 65_536;
const DEFAULT_MUTATIONS: u64 = 4096;
const DEFAULT_FLOW_OPERATIONS: u64 = 262_144;
const MIN_OBJECTS: u32 = 1024;
const MIN_STATES: usize = 4096;
const MIN_EMISSIONS: usize = 1024;
const MIN_MUTATIONS: usize = 256;

impl FlowLimits {
    pub fn from_flow_operations(flow_operations: usize) -> Self {
        let flow = flow_operations as u64;
        Self {
            objects: u32::try_from(DEFAULT_OBJECTS * flow / DEFAULT_FLOW_OPERATIONS)
                .unwrap_or(u32::MAX)
                .max(MIN_OBJECTS),
            states: ((DEFAULT_STATES * flow / DEFAULT_FLOW_OPERATIONS) as usize).max(MIN_STATES),
            emissions: ((DEFAULT_EMISSIONS * flow / DEFAULT_FLOW_OPERATIONS) as usize)
                .max(MIN_EMISSIONS),
            mutation: ((DEFAULT_MUTATIONS * flow / DEFAULT_FLOW_OPERATIONS) as usize)
                .max(MIN_MUTATIONS),
        }
    }

    pub fn object_limit(&self) -> u32 {
        self.objects
    }

    pub fn state_limit(&self) -> usize {
        self.states
    }

    pub fn emission_limit(&self) -> usize {
        self.emissions
    }

    pub fn mutation_limit(&self) -> usize {
        self.mutation
    }

    #[cfg(test)]
    pub fn test_new(objects: u32, states: usize, emissions: usize, mutation: usize) -> Self {
        Self {
            objects,
            states,
            emissions,
            mutation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FlowId {
    rule_index: RuleIndex,
    flow_index: usize,
}

impl FlowId {
    pub fn new(rule_index: RuleIndex, flow_index: usize) -> Self {
        Self {
            rule_index,
            flow_index,
        }
    }

    pub fn rule_index(self) -> RuleIndex {
        self.rule_index
    }

    pub fn flow_index(self) -> usize {
        self.flow_index
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct RequirementSet<K = FactId>(Arc<BTreeMap<usize, K>>);

impl<K: Hash> Hash for RequirementSet<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for (k, v) in self.0.iter() {
            k.hash(state);
            v.hash(state);
        }
    }
}

impl<K> Default for RequirementSet<K> {
    fn default() -> Self {
        Self(Arc::new(BTreeMap::new()))
    }
}

impl<K: Clone> RequirementSet<K> {
    pub fn insert(&mut self, parameter: usize, value: K) {
        Arc::make_mut(&mut self.0).insert(parameter, value);
    }

    pub fn remove(&mut self, parameter: usize) {
        Arc::make_mut(&mut self.0).remove(&parameter);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn values(&self) -> impl Iterator<Item = &K> {
        self.0.values()
    }

    pub fn intersect_keys(&mut self, other: &Self) {
        let map = Arc::make_mut(&mut self.0);
        map.retain(|parameter, _| other.0.contains_key(parameter));
    }
}

use crate::analysis::model::scope::FunctionId;

pub type FunctionTable<T> = glass_lint_datastructures::IndexTable<FunctionId, T>;

use crate::analysis::model::value::ObjectId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowState {
    flow: FlowId,
    source_event: FactId,
    object_id: ObjectId,
    requirements: RequirementSet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FlowStateKey {
    pub object: ObjectId,
    pub flow: FlowId,
}

impl FlowState {
    pub fn new(flow: FlowId, source_event: FactId, object_id: ObjectId) -> Self {
        Self {
            flow,
            source_event,
            object_id,
            requirements: RequirementSet::default(),
        }
    }

    pub fn key(&self) -> FlowStateKey {
        FlowStateKey {
            object: self.object_id,
            flow: self.flow,
        }
    }

    pub fn flow_id(&self) -> FlowId {
        self.flow
    }

    pub fn object_id(&self) -> ObjectId {
        self.object_id
    }

    pub fn source_event(&self) -> FactId {
        self.source_event
    }

    pub fn record_requirement(&mut self, index: usize, event: FactId) {
        self.requirements.insert(index, event);
    }

    pub fn clear_requirement(&mut self, index: usize) {
        self.requirements.remove(index);
    }

    pub fn is_ready(&self, flow: &CompiledObjectFlow) -> bool {
        if flow.all_requirements_required {
            self.requirements.len() == flow.requirements.len()
        } else {
            !self.requirements.is_empty()
        }
    }

    pub fn retain_requirement_keys(&mut self, other: &Self) {
        self.requirements.intersect_keys(&other.requirements);
    }
}
