//! Typed occurrence storage and deterministic normalization.
//!
//! Occurrences are sorted by semantic fact identity and source span, then
//! deduplicated within each key. Queries can therefore borrow stable slices
//! and emit evidence without repeating normalization policy.

use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
};

use swc_common::Span;

use super::super::facts::FactId;

/// Typed occurrence storage. Keeping insertion and normalization in one
/// container prevents semantic collectors from inventing subtly different
/// span ordering or duplicate policies for each provenance view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) struct Occurrence {
    /// Canonical semantic event identity.
    event: FactId,
    /// Source span used for evidence rendering and tie-breaking.
    span: Span,
}

impl Occurrence {
    /// Construct one typed event/span occurrence.
    pub(super) fn new(event: FactId, span: Span) -> Self {
        Self { event, span }
    }

    /// Return the canonical event identity.
    pub(super) fn event(&self) -> FactId {
        self.event
    }

    /// Return the source span associated with the event.
    pub(super) fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Debug, Default)]
/// Ordered occurrence buckets keyed by a typed semantic identity.
pub(in crate::analysis) struct OccurrenceIndex<K: Ord>(BTreeMap<K, Vec<Occurrence>>);

impl<K: Ord> OccurrenceIndex<K> {
    /// Append an already constructed occurrence before normalization.
    pub(super) fn push_occurrence(&mut self, key: K, occurrence: Occurrence) {
        self.0.entry(key).or_default().push(occurrence);
    }

    /// Append one event/span pair before normalization.
    pub(super) fn push(&mut self, key: K, event: FactId, span: Span) {
        self.push_occurrence(key, Occurrence::new(event, span));
    }

    /// Sort and deduplicate every key bucket deterministically.
    pub(super) fn normalize(&mut self) {
        for occurrences in self.0.values_mut() {
            occurrences.sort_by_key(|occurrence| {
                (occurrence.event, occurrence.span.lo, occurrence.span.hi)
            });
            occurrences.dedup();
        }
    }

    /// Merge another index and normalize the combined buckets.
    #[allow(dead_code)]
    pub(super) fn merge(&mut self, other: Self) {
        for (key, occurrences) in other.0 {
            self.0.entry(key).or_default().extend(occurrences);
        }
        self.normalize();
    }

    /// Borrow one normalized occurrence bucket, or an empty slice if absent.
    #[allow(dead_code)]
    pub(super) fn occurrences(&self, key: &K) -> &[Occurrence] {
        self.0.get(key).map_or(&[], Vec::as_slice)
    }
}

impl<K: Ord + Clone> OccurrenceIndex<K> {
    pub(super) fn remap_keys<F>(&mut self, mut remap: F)
    where
        F: FnMut(&K) -> Option<K>,
    {
        let previous = std::mem::take(&mut self.0);
        for (key, occurrences) in previous {
            if let Some(key) = remap(&key) {
                self.0.entry(key).or_default().extend(occurrences);
            }
        }
    }
}

impl<K: Ord> Deref for OccurrenceIndex<K> {
    type Target = BTreeMap<K, Vec<Occurrence>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K: Ord> DerefMut for OccurrenceIndex<K> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub(in crate::analysis) type Occurrences = OccurrenceIndex<String>;
pub(in crate::analysis) type ModuleOccurrences = OccurrenceIndex<(String, String)>;
