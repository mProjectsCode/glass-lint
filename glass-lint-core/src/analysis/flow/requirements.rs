//! Requirement joins used by local and cross-module flow.

use std::collections::BTreeMap;

use super::super::facts::FactId;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub(super) struct RequirementSet<K = FactId>(BTreeMap<usize, K>);

impl<K> Default for RequirementSet<K> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

#[allow(dead_code)]
impl<K: Clone + PartialEq> RequirementSet<K> {
    pub(super) fn record(&mut self, parameter: usize, value: K) {
        self.0.entry(parameter).or_insert(value);
    }

    pub(super) fn insert(&mut self, parameter: usize, value: K) {
        self.0.insert(parameter, value);
    }

    pub(super) fn remove(&mut self, parameter: usize) {
        self.0.remove(&parameter);
    }

    pub(super) fn contains_key(&self, parameter: usize) -> bool {
        self.0.contains_key(&parameter)
    }

    pub(super) fn retain(&mut self, keep: impl FnMut(&usize, &mut K) -> bool) {
        self.0.retain(keep);
    }

    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&usize, &K)> {
        self.0.iter()
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &K> {
        self.0.values()
    }

    pub(super) fn intersect(&mut self, other: &Self) {
        self.0
            .retain(|parameter, fact| other.0.get(parameter) == Some(fact));
    }

    /// Join control-flow paths by retaining requirements proven on both paths.
    /// The evidence value is path-local, so only the typed requirement key is
    /// part of this intersection.
    pub(super) fn intersect_keys(&mut self, other: &Self) {
        self.0
            .retain(|parameter, _| other.0.contains_key(parameter));
    }
}
