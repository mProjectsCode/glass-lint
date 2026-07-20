//! Requirement joins used by local and cross-module flow.

use std::collections::BTreeMap;

use super::super::facts::FactId;

use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
/// Parameter-indexed requirements proven along the current flow path.
///
/// The map is intentionally typed by `K` so local fact IDs and qualified
/// module events cannot be confused during joins.
pub(super) struct RequirementSet<K = FactId>(BTreeMap<usize, K>);

impl<K: Hash> Hash for RequirementSet<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for (k, v) in &self.0 {
            k.hash(state);
            v.hash(state);
        }
    }
}

impl<K> Default for RequirementSet<K> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: PartialEq> RequirementSet<K> {
    /// Replace the proof for a requirement key.
    pub(super) fn insert(&mut self, parameter: usize, value: K) {
        self.0.insert(parameter, value);
    }

    /// Remove one parameter requirement after invalidation.
    pub(super) fn remove(&mut self, parameter: usize) {
        self.0.remove(&parameter);
    }

    /// Number of currently proven requirements.
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether no requirements are currently proven.
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate only the typed proofs in parameter-key order.
    pub(super) fn values(&self) -> impl Iterator<Item = &K> {
        self.0.values()
    }

    /// Join control-flow paths by retaining requirements proven on both paths.
    /// The evidence value is path-local, so only the typed requirement key is
    /// part of this intersection.
    pub(super) fn intersect_keys(&mut self, other: &Self) {
        self.0
            .retain(|parameter, _| other.0.contains_key(parameter));
    }
}
