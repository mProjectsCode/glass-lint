//! Typed occurrence storage and deterministic normalization.

use super::super::facts::FactId;
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
};
use swc_common::Span;

/// Typed occurrence storage. Keeping insertion and normalization in one
/// container prevents semantic collectors from inventing subtly different
/// span ordering or duplicate policies for each provenance view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) struct Occurrence {
    event: FactId,
    span: Span,
}

impl Occurrence {
    pub(super) fn new(event: FactId, span: Span) -> Self {
        Self { event, span }
    }
    pub(super) fn event(&self) -> FactId {
        self.event
    }
    pub(super) fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct OccurrenceIndex<K: Ord>(BTreeMap<K, Vec<Occurrence>>);

#[allow(dead_code)]
impl<K: Ord> OccurrenceIndex<K> {
    pub(super) fn push_occurrence(&mut self, key: K, occurrence: Occurrence) {
        self.0.entry(key).or_default().push(occurrence);
    }

    pub(super) fn push(&mut self, key: K, event: FactId, span: Span) {
        self.push_occurrence(key, Occurrence::new(event, span));
    }

    pub(super) fn normalize(&mut self) {
        for occurrences in self.0.values_mut() {
            occurrences.sort_by_key(|occurrence| {
                (occurrence.event, occurrence.span.lo, occurrence.span.hi)
            });
            occurrences.dedup();
        }
    }

    pub(super) fn merge(&mut self, other: Self) {
        for (key, occurrences) in other.0 {
            self.0.entry(key).or_default().extend(occurrences);
        }
        self.normalize();
    }

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
