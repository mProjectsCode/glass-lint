//! Requirement joins used by local and cross-module flow.

use std::collections::BTreeMap;

use super::super::facts::FactId;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
/// Parameter-indexed requirements proven along the current flow path.
///
/// The map is intentionally typed by `K` so local fact IDs and qualified
/// module events cannot be confused during joins.
pub(super) struct RequirementSet<K = FactId>(BTreeMap<usize, K>);

impl<K> Default for RequirementSet<K> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Clone + PartialEq> RequirementSet<K> {
    /// Record a requirement without replacing an earlier proof for the key.
    #[allow(dead_code)]
    pub(super) fn record(&mut self, parameter: usize, value: K) {
        self.0.entry(parameter).or_insert(value);
    }

    /// Replace the proof for a requirement key.
    pub(super) fn insert(&mut self, parameter: usize, value: K) {
        self.0.insert(parameter, value);
    }

    /// Remove one parameter requirement after invalidation.
    pub(super) fn remove(&mut self, parameter: usize) {
        self.0.remove(&parameter);
    }

    /// Check whether a requirement key is present.
    #[allow(dead_code)]
    pub(super) fn contains_key(&self, parameter: usize) -> bool {
        self.0.contains_key(&parameter)
    }

    /// Retain only requirements satisfying the supplied path predicate.
    #[allow(dead_code)]
    pub(super) fn retain(&mut self, keep: impl FnMut(&usize, &mut K) -> bool) {
        self.0.retain(keep);
    }

    /// Number of currently proven requirements.
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether no requirements are currently proven.
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate parameter keys and their typed proofs in key order.
    #[allow(dead_code)]
    pub(super) fn iter(&self) -> impl Iterator<Item = (&usize, &K)> {
        self.0.iter()
    }

    /// Iterate only the typed proofs in parameter-key order.
    pub(super) fn values(&self) -> impl Iterator<Item = &K> {
        self.0.values()
    }

    #[allow(dead_code)]
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
