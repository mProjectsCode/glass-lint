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

    pub fn requirement_count(&self) -> usize {
        self.requirements.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(value: usize) -> RuleIndex {
        RuleIndex::new(value)
    }

    #[test]
    fn flow_limits_defaults_scale_from_flow_operations() {
        let limits = FlowLimits::from_flow_operations(262_144);
        assert!(limits.object_limit() >= 1024);
        assert!(limits.state_limit() >= 4096);
        assert!(limits.emission_limit() >= 1024);
        assert!(limits.mutation_limit() >= 256);
    }

    #[test]
    fn flow_limits_scales_down_to_minimums() {
        let limits = FlowLimits::from_flow_operations(1);
        assert_eq!(limits.object_limit(), 1024);
        assert_eq!(limits.state_limit(), 4096);
        assert_eq!(limits.emission_limit(), 1024);
        assert_eq!(limits.mutation_limit(), 256);
    }

    #[test]
    fn flow_limits_accessors_return_configured_values() {
        let limits = FlowLimits::test_new(2048, 8192, 2048, 512);
        assert_eq!(limits.object_limit(), 2048);
        assert_eq!(limits.state_limit(), 8192);
        assert_eq!(limits.emission_limit(), 2048);
        assert_eq!(limits.mutation_limit(), 512);
    }

    #[test]
    fn flow_id_new_creates_deterministic_identity() {
        let rule = index(5);
        let a = FlowId::new(rule, 3);
        let b = FlowId::new(rule, 3);
        assert_eq!(a, b);
        assert_eq!(a.rule_index(), rule);
        assert_eq!(a.flow_index(), 3);
    }

    #[test]
    fn flow_id_distinguishes_different_rules_and_indices() {
        let a = FlowId::new(index(1), 2);
        let b = FlowId::new(index(1), 3);
        let c = FlowId::new(index(2), 2);
        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn requirement_set_default_is_empty() {
        let set: RequirementSet = RequirementSet::default();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn requirement_set_insert_and_remove() {
        let mut set: RequirementSet = RequirementSet::default();
        set.insert(0, FactId(1));
        set.insert(1, FactId(2));
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());

        set.remove(0);
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn requirement_set_values_returns_all_inserted() {
        let mut set: RequirementSet = RequirementSet::default();
        set.insert(0, FactId(10));
        set.insert(2, FactId(30));
        let values: Vec<_> = set.values().copied().collect();
        assert_eq!(values, vec![FactId(10), FactId(30)]);
    }

    #[test]
    fn requirement_set_intersect_keys_keeps_common_parameters() {
        let mut a: RequirementSet = RequirementSet::default();
        a.insert(0, FactId(1));
        a.insert(1, FactId(2));
        a.insert(2, FactId(3));
        let mut b: RequirementSet = RequirementSet::default();
        b.insert(0, FactId(10));
        b.insert(2, FactId(30));

        a.intersect_keys(&b);
        assert_eq!(a.len(), 2);
        assert!(a.values().any(|&v| v == FactId(1)));
        assert!(a.values().any(|&v| v == FactId(3)));
    }

    #[test]
    fn flow_state_new_creates_unready_state() {
        let flow = FlowId::new(index(0), 0);
        let state = FlowState::new(flow, FactId(1), ObjectId(0));
        assert_eq!(state.flow_id(), flow);
        assert_eq!(state.source_event(), FactId(1));
        assert_eq!(state.object_id(), ObjectId(0));
    }

    #[test]
    fn flow_state_key_matches_flow_and_object() {
        let flow = FlowId::new(index(1), 2);
        let state = FlowState::new(flow, FactId(5), ObjectId(3));
        let key = state.key();
        assert_eq!(key.object, ObjectId(3));
        assert_eq!(key.flow, flow);
    }

    #[test]
    fn flow_state_records_and_clears_requirements() {
        let flow = FlowId::new(index(0), 0);
        let mut state = FlowState::new(flow, FactId(1), ObjectId(0));
        state.record_requirement(0, FactId(10));
        state.record_requirement(1, FactId(20));
        assert_eq!(state.requirements.len(), 2);

        state.clear_requirement(0);
        assert_eq!(state.requirements.len(), 1);
    }

    #[test]
    fn flow_state_retain_requirement_keys_intersects() {
        let flow = FlowId::new(index(0), 0);
        let mut a = FlowState::new(flow, FactId(1), ObjectId(0));
        a.record_requirement(0, FactId(10));
        a.record_requirement(1, FactId(20));

        let mut b = FlowState::new(flow, FactId(2), ObjectId(0));
        b.record_requirement(0, FactId(30));

        a.retain_requirement_keys(&b);
        assert_eq!(a.requirements.len(), 1);
    }
}
