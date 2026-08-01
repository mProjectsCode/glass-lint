use std::{
    collections::BTreeSet,
    hash::{Hash, Hasher},
};

use smallvec::SmallVec;

use crate::{
    analysis::model::{fact::FactId, scope::FunctionId, value::ObjectId},
    api::{
        classification::RuleIndex,
        compiler::{CompiledObjectFlow, object_flow::RequirementMode},
    },
};

pub type FunctionTable<T> = glass_lint_datastructures::IndexTable<FunctionId, T>;

#[derive(Debug, Clone, Copy)]
pub struct FlowLimits {
    objects: u32,
    states: usize,
    emissions: usize,
    mutation: usize,
    alternatives: usize,
    operations: usize,
    local_operations: usize,
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
const DEFAULT_ALTERNATIVES: usize = 4096;
const MIN_ALTERNATIVES: usize = 16;

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
            alternatives: ((DEFAULT_ALTERNATIVES as u64 * flow / DEFAULT_FLOW_OPERATIONS) as usize)
                .max(MIN_ALTERNATIVES),
            operations: flow_operations,
            // Local projection owns one budget per module; cross-file
            // propagation owns one budget for the project phase.
            local_operations: flow_operations,
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

    pub fn alternative_limit(&self) -> usize {
        self.alternatives
    }

    /// Maximum number of charged local flow operations.
    pub fn operation_limit(&self) -> usize {
        self.operations
    }

    pub fn local_operation_limit(&self) -> usize {
        self.local_operations
    }

    #[cfg(test)]
    pub fn test_new(objects: u32, states: usize, emissions: usize, mutation: usize) -> Self {
        Self {
            objects,
            states,
            emissions,
            mutation,
            alternatives: states.max(1),
            operations: usize::MAX,
            local_operations: usize::MAX,
        }
    }

    #[cfg(test)]
    pub fn test_with_operation_limit(
        objects: u32,
        states: usize,
        emissions: usize,
        mutation: usize,
        operations: usize,
    ) -> Self {
        Self {
            objects,
            states,
            emissions,
            mutation,
            alternatives: states.max(1),
            operations,
            local_operations: operations,
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

pub type RequirementValues<K> = SmallVec<[K; 1]>;

/// Typed index of a lifecycle requirement in one compiled flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequirementIndex(usize);

impl RequirementIndex {
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn get(self) -> usize {
        self.0
    }
}

impl From<RequirementIndex> for usize {
    fn from(index: RequirementIndex) -> Self {
        index.0
    }
}

/// Typed index of a lifecycle sink in one compiled flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SinkIndex(usize);

impl SinkIndex {
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn get(self) -> usize {
        self.0
    }
}

impl From<SinkIndex> for usize {
    fn from(index: SinkIndex) -> Self {
        index.0
    }
}

pub trait EvidenceIndex: Copy + Ord + Hash + Into<usize> {}

impl EvidenceIndex for usize {}
impl EvidenceIndex for RequirementIndex {}
impl EvidenceIndex for SinkIndex {}

/// Bounded completion evidence keyed by lifecycle requirement index.
///
/// The mask owns readiness checks; the sorted compact entries retain the
/// deterministic evidence needed for traces. Lifecycle declarations cap the
/// key domain at 64, so a tree-backed map adds copy-on-write and node-walk
/// overhead without providing a useful invariant.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct RequirementSet<K = FactId, I = usize> {
    mask: u64,
    entries: SmallVec<[(I, RequirementValues<K>); 4]>,
}

impl<K: Hash, I: EvidenceIndex> Hash for RequirementSet<K, I> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.mask.hash(state);
        for (k, vals) in &self.entries {
            k.hash(state);
            for v in vals {
                v.hash(state);
            }
        }
    }
}

impl<K, I: EvidenceIndex> Default for RequirementSet<K, I> {
    fn default() -> Self {
        Self {
            mask: 0,
            entries: SmallVec::new(),
        }
    }
}

impl<K: Clone + Ord, I: EvidenceIndex> RequirementSet<K, I> {
    pub fn insert(&mut self, parameter: I, value: K) -> bool {
        let Some(bit) = Self::bit(parameter) else {
            return false;
        };
        match self
            .entries
            .binary_search_by_key(&parameter, |(index, _)| *index)
        {
            Ok(position) => {
                let values = &mut self.entries[position].1;
                match values.binary_search(&value) {
                    Ok(_) => false,
                    Err(position) => {
                        values.insert(position, value);
                        true
                    }
                }
            }
            Err(position) => {
                let mut values = RequirementValues::new();
                values.push(value);
                self.entries.insert(position, (parameter, values));
                self.mask |= bit;
                true
            }
        }
    }

    pub fn remove(&mut self, parameter: I) -> Option<RequirementValues<K>> {
        let position = self
            .entries
            .binary_search_by_key(&parameter, |(index, _)| *index)
            .ok()?;
        self.mask &= !Self::bit(parameter).expect("stored requirement index is bounded");
        Some(self.entries.remove(position).1)
    }

    pub fn remove_value(&mut self, parameter: I, value: &K) -> bool {
        let Ok(position) = self
            .entries
            .binary_search_by_key(&parameter, |(index, _)| *index)
        else {
            return false;
        };
        let Ok(value_position) = self.entries[position].1.binary_search(value) else {
            return false;
        };
        self.entries[position].1.remove(value_position);
        if self.entries[position].1.is_empty() {
            self.entries.remove(position);
            self.mask &= !Self::bit(parameter).expect("stored requirement index is bounded");
        }
        true
    }

    pub fn restore<Values>(&mut self, parameter: I, values: Values)
    where
        Values: IntoIterator<Item = K>,
    {
        for value in values {
            self.insert(parameter, value);
        }
    }

    pub fn len(&self) -> usize {
        self.mask.count_ones() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.mask == 0
    }

    pub fn values(&self) -> impl Iterator<Item = &K> {
        self.entries.iter().flat_map(|(_, values)| values.iter())
    }

    pub fn iter_by_key(&self) -> impl Iterator<Item = (I, &RequirementValues<K>)> {
        self.entries.iter().map(|(index, values)| (*index, values))
    }

    fn bit(parameter: I) -> Option<u64> {
        let parameter = parameter.into();
        (parameter < u64::BITS as usize).then(|| 1u64 << parameter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowState {
    flow: FlowId,
    source_event: FactId,
    object_id: ObjectId,
    requirements: RequirementSet<FactId, RequirementIndex>,
    sinks: RequirementSet<FactId, SinkIndex>,
}

impl Hash for FlowState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.flow.hash(state);
        self.source_event.hash(state);
        self.object_id.hash(state);
        self.requirements.hash(state);
        self.sinks.hash(state);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
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
            sinks: RequirementSet::default(),
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

    pub fn record_requirement(&mut self, index: RequirementIndex, event: FactId) -> bool {
        self.requirements.insert(index, event)
    }

    pub fn clear_requirement(&mut self, index: RequirementIndex) -> Option<BTreeSet<FactId>> {
        self.requirements
            .remove(index)
            .map(|events| events.into_iter().collect())
    }

    pub fn remove_requirement_event(&mut self, index: RequirementIndex, event: FactId) -> bool {
        self.requirements.remove_value(index, &event)
    }

    pub fn restore_requirement(&mut self, index: RequirementIndex, events: &BTreeSet<FactId>) {
        self.requirements.restore(index, events.iter().copied());
    }

    pub fn is_ready(&self, flow: &CompiledObjectFlow) -> bool {
        match flow.requirement_mode {
            RequirementMode::AllRequired => self.requirements.len() == flow.requirements.len(),
            RequirementMode::AnyRequired => !self.requirements.is_empty(),
        }
    }

    pub fn record_sink(&mut self, index: SinkIndex, event: FactId) -> bool {
        self.sinks.insert(index, event)
    }

    pub fn remove_sink_event(&mut self, index: SinkIndex, event: FactId) -> bool {
        self.sinks.remove_value(index, &event)
    }

    pub fn sinks_ready(&self, flow: &CompiledObjectFlow) -> bool {
        flow.completion_mode != crate::api::compiler::object_flow::CompletionMode::AllSinks
            || self.sinks.len() == flow.sinks.len()
    }

    pub fn requirement_keys(
        &self,
    ) -> impl Iterator<Item = (RequirementIndex, &RequirementValues<FactId>)> {
        self.requirements.iter_by_key()
    }

    pub fn sink_keys(&self) -> impl Iterator<Item = (SinkIndex, &RequirementValues<FactId>)> {
        self.sinks.iter_by_key()
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
    fn flow_operation_limit_tracks_the_configured_budget() {
        let limits = FlowLimits::from_flow_operations(1234);
        assert_eq!(limits.operation_limit(), 1234);
        assert_eq!(limits.local_operation_limit(), 1234);
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
        set.insert(0, FactId::from_test(1));
        set.insert(1, FactId::from_test(2));
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());

        set.remove(0);
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn requirement_set_values_returns_all_inserted() {
        let mut set: RequirementSet = RequirementSet::default();
        set.insert(0, FactId::from_test(10));
        set.insert(2, FactId::from_test(30));
        let values: Vec<_> = set.values().copied().collect();
        assert_eq!(values, vec![FactId::from_test(10), FactId::from_test(30)]);
    }

    #[test]
    fn requirement_set_insert_duplicate_key_appends_value() {
        let mut set: RequirementSet = RequirementSet::default();
        set.insert(0, FactId::from_test(10));
        set.insert(0, FactId::from_test(20));
        let values: Vec<_> = set.values().copied().collect();
        assert_eq!(values.len(), 2);
        assert!(values.contains(&FactId::from_test(10)));
        assert!(values.contains(&FactId::from_test(20)));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn requirement_set_uses_all_64_completion_bits_and_rejects_overflow() {
        let mut set: RequirementSet = RequirementSet::default();
        assert!(set.insert(63, FactId::from_test(63)));
        assert!(!set.insert(64, FactId::from_test(64)));
        assert_eq!(set.len(), 1);
        assert_eq!(
            set.values().copied().collect::<Vec<_>>(),
            [FactId::from_test(63)]
        );
    }

    #[test]
    fn requirement_and_sink_indices_preserve_their_domains() {
        let mut requirements: RequirementSet<FactId, RequirementIndex> = RequirementSet::default();
        let mut sinks: RequirementSet<FactId, SinkIndex> = RequirementSet::default();
        assert!(requirements.insert(RequirementIndex::new(63), FactId::from_test(63)));
        assert!(sinks.insert(SinkIndex::new(63), FactId::from_test(63)));
        assert_eq!(
            requirements
                .iter_by_key()
                .find(|(index, _)| *index == RequirementIndex::new(63))
                .map(|(_, values)| values.len()),
            Some(1)
        );
        assert_eq!(
            sinks
                .iter_by_key()
                .find(|(index, _)| *index == SinkIndex::new(63))
                .map(|(_, values)| values.len()),
            Some(1)
        );
        assert!(!requirements.insert(RequirementIndex::new(64), FactId::from_test(64)));
        assert!(!sinks.insert(SinkIndex::new(64), FactId::from_test(64)));
    }

    #[test]
    fn flow_state_new_creates_unready_state() {
        let flow = FlowId::new(index(0), 0);
        let state = FlowState::new(flow, FactId::from_test(1), ObjectId::from_test(0));
        assert_eq!(state.flow_id(), flow);
        assert_eq!(state.source_event(), FactId::from_test(1));
        assert_eq!(state.object_id(), ObjectId::from_test(0));
    }

    #[test]
    fn flow_state_key_matches_flow_and_object() {
        let flow = FlowId::new(index(1), 2);
        let state = FlowState::new(flow, FactId::from_test(5), ObjectId::from_test(3));
        let key = state.key();
        assert_eq!(key.object, ObjectId::from_test(3));
        assert_eq!(key.flow, flow);
    }

    #[test]
    fn flow_state_records_and_clears_requirements() {
        let flow = FlowId::new(index(0), 0);
        let mut state = FlowState::new(flow, FactId::from_test(1), ObjectId::from_test(0));
        state.record_requirement(RequirementIndex::new(0), FactId::from_test(10));
        state.record_requirement(RequirementIndex::new(1), FactId::from_test(20));
        assert_eq!(state.requirements.len(), 2);

        state.clear_requirement(RequirementIndex::new(0));
        assert_eq!(state.requirements.len(), 1);
    }
}
