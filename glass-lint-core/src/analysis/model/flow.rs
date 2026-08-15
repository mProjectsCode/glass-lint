use std::hash::{Hash, Hasher};

use smallvec::SmallVec;

use crate::{
    analysis::model::{fact::FactId, scope::FunctionId, value::FlowObjectId},
    api::classification::RuleIndex,
};

pub(in crate::analysis) type FunctionTable<T> =
    glass_lint_datastructures::IndexTable<FunctionId, T>;

mod limits;
mod state;

pub(in crate::analysis) use limits::FlowLimits;
pub(in crate::analysis) use state::{FlowState, FlowStateKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::analysis) struct FlowId {
    rule_index: RuleIndex,
    flow_index: usize,
}

impl FlowId {
    pub(in crate::analysis) fn new(rule_index: RuleIndex, flow_index: usize) -> Self {
        Self {
            rule_index,
            flow_index,
        }
    }

    pub(in crate::analysis) fn rule_index(self) -> RuleIndex {
        self.rule_index
    }

    pub(in crate::analysis) fn flow_index(self) -> usize {
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

/// Bounded lifecycle key domain shared by requirement and sink indices.
///
/// Lifecycle declarations cap the key domain at [`BoundedIndex::MAX`], so each
/// index maps to one bit of the [`IndexedEvidence`] mask. The cap and the mask
/// arithmetic live here so a change to the domain bound is made in one place.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct BoundedIndex(usize);

impl BoundedIndex {
    const MAX: usize = u64::BITS as usize;

    fn new(index: usize) -> Option<Self> {
        (index < Self::MAX).then_some(Self(index))
    }

    fn get(self) -> usize {
        self.0
    }

    fn bit(self) -> u64 {
        1u64 << self.0
    }

    /// The full mask for the first `count` indices, or `None` when `count`
    /// exceeds the bounded key domain.
    fn mask(count: usize) -> Option<u64> {
        if count > Self::MAX {
            return None;
        }
        Some(if count == Self::MAX {
            u64::MAX
        } else {
            (1u64 << count).saturating_sub(1)
        })
    }
}

/// Typed index of a lifecycle requirement in one compiled flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) struct RequirementIndex(BoundedIndex);

impl RequirementIndex {
    pub(in crate::analysis) fn new(index: usize) -> Option<Self> {
        BoundedIndex::new(index).map(Self)
    }
}

impl From<RequirementIndex> for usize {
    fn from(index: RequirementIndex) -> Self {
        index.0.get()
    }
}

/// Typed index of a lifecycle sink in one compiled flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) struct SinkIndex(BoundedIndex);

impl SinkIndex {
    pub(in crate::analysis) fn new(index: usize) -> Option<Self> {
        BoundedIndex::new(index).map(Self)
    }
}

impl From<SinkIndex> for usize {
    fn from(index: SinkIndex) -> Self {
        index.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum RequirementReadiness {
    Any,
    All,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum SinkReadiness {
    Configuration,
    Any,
    All,
}

/// Compiler-independent lifecycle completion policy lowered at the analysis
/// boundary from matcher declarations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct FlowReadiness {
    requirement_mode: RequirementReadiness,
    requirement_count: usize,
    sink_mode: SinkReadiness,
    sink_count: usize,
}

impl FlowReadiness {
    pub(crate) const fn new(
        requirement_mode: RequirementReadiness,
        requirement_count: usize,
        sink_mode: SinkReadiness,
        sink_count: usize,
    ) -> Self {
        Self {
            requirement_mode,
            requirement_count,
            sink_mode,
            sink_count,
        }
    }
}

trait EvidenceIndex: Copy + Ord + Hash + Into<usize> {
    fn bounded(self) -> BoundedIndex;
}

impl EvidenceIndex for RequirementIndex {
    fn bounded(self) -> BoundedIndex {
        self.0
    }
}

impl EvidenceIndex for SinkIndex {
    fn bounded(self) -> BoundedIndex {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub(in crate::analysis) struct LifecycleRollback<E>(EvidenceValues<E>);

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

    #[cfg(test)]
    pub fn key_count(&self) -> usize {
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

    fn ready_any(&self, count: usize) -> bool {
        if self
            .entries
            .iter()
            .any(|(index, _)| (*index).bounded().get() >= count)
        {
            return false;
        }
        self.mask != 0
    }

    fn ready_all(&self, count: usize) -> bool {
        if self
            .entries
            .iter()
            .any(|(index, _)| (*index).bounded().get() >= count)
        {
            return false;
        }
        let Some(required) = BoundedIndex::mask(count) else {
            return false;
        };
        self.mask == required
    }

    fn bit(parameter: I) -> u64 {
        parameter.bounded().bit()
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

    pub(in crate::analysis) fn requirements_ready(&self, readiness: FlowReadiness) -> bool {
        match readiness.requirement_mode {
            RequirementReadiness::Any => self.requirements.ready_any(readiness.requirement_count),
            RequirementReadiness::All => self.requirements.ready_all(readiness.requirement_count),
        }
    }

    pub(in crate::analysis) fn record_sink(&mut self, index: SinkIndex, event: E) -> bool {
        self.sinks.insert(index, event)
    }

    pub(in crate::analysis) fn remove_sink_event(&mut self, index: SinkIndex, event: &E) -> bool {
        self.sinks.remove_value(index, event)
    }

    pub(in crate::analysis) fn sinks_ready(&self, readiness: FlowReadiness) -> bool {
        match readiness.sink_mode {
            SinkReadiness::Configuration | SinkReadiness::Any => true,
            SinkReadiness::All => self.sinks.ready_all(readiness.sink_count),
        }
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

    /// First event recorded for each requirement index, in declaration order.
    ///
    /// The trace consumer only needs the first event of each index, so this
    /// borrows instead of materializing a `Vec` per index.
    pub(in crate::analysis) fn first_requirement_events(
        &self,
    ) -> impl Iterator<Item = (RequirementIndex, &E)> {
        self.requirements
            .iter_by_key()
            .filter_map(|(index, values)| values.iter().next().map(|event| (index, event)))
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

#[cfg(test)]
mod tests;
