use std::hash::{Hash, Hasher};

use smallvec::SmallVec;

use crate::{
    analysis::model::{fact::FactId, scope::FunctionId, value::ObjectId},
    api::{classification::RuleIndex, compiler::CompiledObjectFlow},
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
        Self {
            objects: u32::try_from(scaled_limit(
                DEFAULT_OBJECTS,
                flow_operations,
                MIN_OBJECTS as usize,
                u32::MAX as usize,
            ))
            .unwrap_or(u32::MAX),
            states: scaled_limit(DEFAULT_STATES, flow_operations, MIN_STATES, usize::MAX),
            emissions: scaled_limit(
                DEFAULT_EMISSIONS,
                flow_operations,
                MIN_EMISSIONS,
                usize::MAX,
            ),
            mutation: scaled_limit(
                DEFAULT_MUTATIONS,
                flow_operations,
                MIN_MUTATIONS,
                usize::MAX,
            ),
            alternatives: scaled_limit(
                DEFAULT_ALTERNATIVES as u64,
                flow_operations,
                MIN_ALTERNATIVES,
                usize::MAX,
            ),
            operations: flow_operations,
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

    /// Maximum number of charged operations for one flow scope.
    pub fn operation_limit(&self) -> usize {
        self.operations
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
        }
    }
}

fn scaled_limit(default: u64, flow_operations: usize, minimum: usize, maximum: usize) -> usize {
    let flow = u64::try_from(flow_operations).unwrap_or(u64::MAX);
    let scaled = default
        .checked_mul(flow)
        .map_or(u64::MAX, |product| product / DEFAULT_FLOW_OPERATIONS);
    usize::try_from(scaled)
        .unwrap_or(usize::MAX)
        .clamp(minimum, maximum)
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

/// Sorted evidence values recorded for one lifecycle index.
///
/// The compact vector storage stays private so callers cannot depend on the
/// representation; removal deltas and restore transitions both use this type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Ord, PartialOrd)]
struct EvidenceValues<K>(SmallVec<[K; 1]>);

impl<K> EvidenceValues<K> {
    fn new() -> Self {
        Self(SmallVec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.0.iter()
    }
}

impl<K: Ord> EvidenceValues<K> {
    fn from_value(value: K) -> Self {
        let mut values = Self::new();
        values.insert(value);
        values
    }

    fn insert(&mut self, value: K) -> bool {
        match self.0.binary_search(&value) {
            Ok(_) => false,
            Err(position) => {
                self.0.insert(position, value);
                true
            }
        }
    }

    fn remove(&mut self, value: &K) -> bool {
        match self.0.binary_search(value) {
            Ok(position) => {
                self.0.remove(position);
                true
            }
            Err(_) => false,
        }
    }
}

/// Typed index of a lifecycle requirement in one compiled flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequirementIndex(usize);

impl RequirementIndex {
    pub fn new(index: usize) -> Option<Self> {
        (index < u64::BITS as usize).then_some(Self(index))
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
    pub fn new(index: usize) -> Option<Self> {
        (index < u64::BITS as usize).then_some(Self(index))
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

trait EvidenceIndex: Copy + Ord + Hash + Into<usize> {}

impl EvidenceIndex for RequirementIndex {}
impl EvidenceIndex for SinkIndex {}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct LifecycleRollback<E>(EvidenceValues<E>);

/// Bounded indexed evidence for lifecycle requirements and sinks.
///
/// The mask owns readiness checks; the sorted compact entries retain the
/// deterministic evidence needed for traces. Lifecycle declarations cap the
/// key domain at 64, so a tree-backed map adds copy-on-write and node-walk
/// overhead without providing a useful invariant. `remove` and `restore`
/// transfer the owner's semantic [`EvidenceValues`] delta so history never
/// re-encodes the values in a second collection.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
struct IndexedEvidence<K, I> {
    mask: u64,
    entries: SmallVec<[(I, EvidenceValues<K>); 4]>,
}

impl<K: Hash, I: EvidenceIndex> Hash for IndexedEvidence<K, I> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.mask.hash(state);
        for (k, vals) in &self.entries {
            k.hash(state);
            for v in vals.iter() {
                v.hash(state);
            }
        }
    }
}

impl<K, I: EvidenceIndex> Default for IndexedEvidence<K, I> {
    fn default() -> Self {
        Self {
            mask: 0,
            entries: SmallVec::new(),
        }
    }
}

impl<K: Clone + Ord, I: EvidenceIndex> IndexedEvidence<K, I> {
    pub fn insert(&mut self, parameter: I, value: K) -> bool {
        let bit = Self::bit(parameter);
        match self
            .entries
            .binary_search_by_key(&parameter, |(index, _)| *index)
        {
            Ok(position) => self.entries[position].1.insert(value),
            Err(position) => {
                self.entries
                    .insert(position, (parameter, EvidenceValues::from_value(value)));
                self.mask |= bit;
                true
            }
        }
    }

    pub fn remove(&mut self, parameter: I) -> Option<EvidenceValues<K>> {
        let position = self
            .entries
            .binary_search_by_key(&parameter, |(index, _)| *index)
            .ok()?;
        self.mask &= !Self::bit(parameter);
        Some(self.entries.remove(position).1)
    }

    pub fn remove_value(&mut self, parameter: I, value: &K) -> bool {
        let Ok(position) = self
            .entries
            .binary_search_by_key(&parameter, |(index, _)| *index)
        else {
            return false;
        };
        if !self.entries[position].1.remove(value) {
            return false;
        }
        if self.entries[position].1.is_empty() {
            self.entries.remove(position);
            self.mask &= !Self::bit(parameter);
        }
        true
    }

    pub fn restore(&mut self, parameter: I, values: &EvidenceValues<K>) {
        for value in values.iter().cloned() {
            let _ = self.insert(parameter, value);
        }
    }

    pub fn len(&self) -> usize {
        self.mask.count_ones() as usize
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.mask == 0
    }

    pub fn values(&self) -> impl Iterator<Item = &K> {
        self.entries.iter().flat_map(|(_, values)| values.iter())
    }

    pub fn iter_by_key(&self) -> impl Iterator<Item = (I, &EvidenceValues<K>)> {
        self.entries.iter().map(|(index, values)| (*index, values))
    }

    fn bit(parameter: I) -> u64 {
        1u64 << parameter.into()
    }
}

/// Shared lifecycle evidence storage for local and qualified flow states.
///
/// The event type stays generic so local fact identity and cross-file
/// qualified identity remain distinct while recording, readiness, and
/// deterministic iteration have one owner.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub(in crate::analysis) struct LifecycleEvidence<E> {
    requirements: IndexedEvidence<E, RequirementIndex>,
    sinks: IndexedEvidence<E, SinkIndex>,
}

impl<E> Default for LifecycleEvidence<E> {
    fn default() -> Self {
        Self {
            requirements: IndexedEvidence::default(),
            sinks: IndexedEvidence::default(),
        }
    }
}

impl<E: Hash> Hash for LifecycleEvidence<E> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.requirements.hash(state);
        self.sinks.hash(state);
    }
}

impl<E: Clone + Ord> LifecycleEvidence<E> {
    pub(in crate::analysis) fn record_requirement(
        &mut self,
        index: RequirementIndex,
        event: E,
    ) -> bool {
        self.requirements.insert(index, event)
    }

    pub(in crate::analysis) fn clear_requirement(
        &mut self,
        index: RequirementIndex,
    ) -> Option<LifecycleRollback<E>> {
        self.requirements.remove(index).map(LifecycleRollback)
    }

    pub(in crate::analysis) fn remove_requirement_event(
        &mut self,
        index: RequirementIndex,
        event: &E,
    ) -> bool {
        self.requirements.remove_value(index, event)
    }

    pub(in crate::analysis) fn restore_requirement(
        &mut self,
        index: RequirementIndex,
        events: &LifecycleRollback<E>,
    ) {
        self.requirements.restore(index, &events.0);
    }

    pub(in crate::analysis) fn requirements_ready(&self, flow: &CompiledObjectFlow) -> bool {
        flow.requirements_ready(self.requirements.len())
    }

    pub(in crate::analysis) fn record_sink(&mut self, index: SinkIndex, event: E) -> bool {
        self.sinks.insert(index, event)
    }

    pub(in crate::analysis) fn remove_sink_event(&mut self, index: SinkIndex, event: &E) -> bool {
        self.sinks.remove_value(index, event)
    }

    pub(in crate::analysis) fn sinks_ready(&self, flow: &CompiledObjectFlow) -> bool {
        flow.completion_mode() != crate::api::compiler::object_flow::CompletionMode::AllSinks
            || self.sinks.len() == flow.sink_count()
    }

    pub(in crate::analysis) fn requirement_entries(
        &self,
    ) -> impl Iterator<Item = (RequirementIndex, Vec<E>)> {
        self.requirements
            .iter_by_key()
            .map(|(index, values)| (index, values.iter().cloned().collect()))
    }

    pub(in crate::analysis) fn sink_entries(&self) -> impl Iterator<Item = (SinkIndex, Vec<E>)> {
        self.sinks
            .iter_by_key()
            .map(|(index, values)| (index, values.iter().cloned().collect()))
    }

    pub(in crate::analysis) fn requirement_events(&self) -> impl Iterator<Item = &E> {
        self.requirements.values()
    }

    pub(in crate::analysis) fn sink_events(&self) -> impl Iterator<Item = &E> {
        self.sinks.values()
    }

    pub(in crate::analysis) fn prior_sink_events(&self, exclude: impl Fn(&E) -> bool) -> Vec<E> {
        let mut events: Vec<_> = self
            .sink_events()
            .filter(|event| !exclude(event))
            .cloned()
            .collect();
        events.sort();
        events.dedup();
        events
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowState {
    flow: FlowId,
    source_event: FactId,
    object_id: ObjectId,
    evidence: LifecycleEvidence<FactId>,
}

impl Hash for FlowState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.flow.hash(state);
        self.source_event.hash(state);
        self.object_id.hash(state);
        self.evidence.hash(state);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct FlowStateKey {
    object: ObjectId,
    flow: FlowId,
}

impl FlowStateKey {
    pub fn new(object: ObjectId, flow: FlowId) -> Self {
        Self { object, flow }
    }

    pub fn object(self) -> ObjectId {
        self.object
    }

    pub fn flow(self) -> FlowId {
        self.flow
    }
}

impl FlowState {
    pub fn new(flow: FlowId, source_event: FactId, object_id: ObjectId) -> Self {
        Self {
            flow,
            source_event,
            object_id,
            evidence: LifecycleEvidence::default(),
        }
    }

    pub fn key(&self) -> FlowStateKey {
        FlowStateKey::new(self.object_id, self.flow)
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
        self.evidence.record_requirement(index, event)
    }

    pub(crate) fn clear_requirement(
        &mut self,
        index: RequirementIndex,
    ) -> Option<LifecycleRollback<FactId>> {
        self.evidence.clear_requirement(index)
    }

    pub fn remove_requirement_event(&mut self, index: RequirementIndex, event: FactId) -> bool {
        self.evidence.remove_requirement_event(index, &event)
    }

    pub(crate) fn restore_requirement(
        &mut self,
        index: RequirementIndex,
        events: &LifecycleRollback<FactId>,
    ) {
        self.evidence.restore_requirement(index, events);
    }

    pub fn is_ready(&self, flow: &CompiledObjectFlow) -> bool {
        self.evidence.requirements_ready(flow)
    }

    pub fn record_sink(&mut self, index: SinkIndex, event: FactId) -> bool {
        self.evidence.record_sink(index, event)
    }

    pub fn remove_sink_event(&mut self, index: SinkIndex, event: FactId) -> bool {
        self.evidence.remove_sink_event(index, &event)
    }

    pub fn sinks_ready(&self, flow: &CompiledObjectFlow) -> bool {
        self.evidence.sinks_ready(flow)
    }

    pub(crate) fn requirement_entries(
        &self,
    ) -> impl Iterator<Item = (RequirementIndex, Vec<FactId>)> {
        self.evidence.requirement_entries()
    }

    pub(crate) fn sink_entries(&self) -> impl Iterator<Item = (SinkIndex, Vec<FactId>)> {
        self.evidence.sink_entries()
    }

    pub(in crate::analysis) fn prior_sinks(&self, event: FactId) -> Vec<FactId> {
        self.evidence.prior_sink_events(|sink| *sink == event)
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
    fn flow_limits_large_operation_budget_does_not_overflow() {
        let limits = FlowLimits::from_flow_operations(usize::MAX);
        assert!(limits.object_limit() >= 1024);
        assert!(limits.state_limit() >= 4096);
        assert!(limits.emission_limit() >= 1024);
        assert!(limits.mutation_limit() >= 256);
        assert!(limits.alternative_limit() >= 16);
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
    fn indexed_evidence_default_is_empty() {
        let set: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn indexed_evidence_insert_and_remove() {
        let mut set: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
        set.insert(RequirementIndex::new(0).unwrap(), FactId::from_test(1));
        set.insert(RequirementIndex::new(1).unwrap(), FactId::from_test(2));
        assert_eq!(set.len(), 2);
        assert!(!set.is_empty());

        set.remove(RequirementIndex::new(0).unwrap());
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn indexed_evidence_values_returns_all_inserted() {
        let mut set: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
        set.insert(RequirementIndex::new(0).unwrap(), FactId::from_test(10));
        set.insert(RequirementIndex::new(2).unwrap(), FactId::from_test(30));
        let values: Vec<_> = set.values().copied().collect();
        assert_eq!(values, vec![FactId::from_test(10), FactId::from_test(30)]);
    }

    #[test]
    fn indexed_evidence_insert_duplicate_key_appends_value() {
        let mut set: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
        set.insert(RequirementIndex::new(0).unwrap(), FactId::from_test(10));
        set.insert(RequirementIndex::new(0).unwrap(), FactId::from_test(20));
        let values: Vec<_> = set.values().copied().collect();
        assert_eq!(values.len(), 2);
        assert!(values.contains(&FactId::from_test(10)));
        assert!(values.contains(&FactId::from_test(20)));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn indexed_evidence_uses_all_64_completion_bits_and_rejects_overflow() {
        let mut set: IndexedEvidence<FactId, RequirementIndex> = IndexedEvidence::default();
        assert!(set.insert(RequirementIndex::new(63).unwrap(), FactId::from_test(63)));
        assert!(RequirementIndex::new(64).is_none());
        assert_eq!(set.len(), 1);
        assert_eq!(
            set.values().copied().collect::<Vec<_>>(),
            [FactId::from_test(63)]
        );
    }

    #[test]
    fn requirement_and_sink_indices_preserve_their_domains() {
        let mut requirements: IndexedEvidence<FactId, RequirementIndex> =
            IndexedEvidence::default();
        let mut sinks: IndexedEvidence<FactId, SinkIndex> = IndexedEvidence::default();
        assert!(requirements.insert(RequirementIndex::new(63).unwrap(), FactId::from_test(63)));
        assert!(sinks.insert(SinkIndex::new(63).unwrap(), FactId::from_test(63)));
        assert_eq!(
            requirements
                .iter_by_key()
                .find(|(index, _)| *index == RequirementIndex::new(63).unwrap())
                .map(|(_, values)| values.iter().count()),
            Some(1)
        );
        assert_eq!(
            sinks
                .iter_by_key()
                .find(|(index, _)| *index == SinkIndex::new(63).unwrap())
                .map(|(_, values)| values.iter().count()),
            Some(1)
        );
        assert!(RequirementIndex::new(64).is_none());
        assert!(SinkIndex::new(64).is_none());
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
        assert_eq!(key.object(), ObjectId::from_test(3));
        assert_eq!(key.flow(), flow);
    }

    #[test]
    fn flow_state_records_and_clears_requirements() {
        let flow = FlowId::new(index(0), 0);
        let mut state = FlowState::new(flow, FactId::from_test(1), ObjectId::from_test(0));
        state.record_requirement(RequirementIndex::new(0).unwrap(), FactId::from_test(10));
        state.record_requirement(RequirementIndex::new(1).unwrap(), FactId::from_test(20));
        assert_eq!(state.requirement_entries().count(), 2);

        state.clear_requirement(RequirementIndex::new(0).unwrap());
        assert_eq!(state.requirement_entries().count(), 1);
    }
}
