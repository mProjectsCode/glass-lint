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
    pub(super) event: FactId,
    pub(super) span: Span,
}

#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct OccurrenceIndex<K: Ord>(BTreeMap<K, Vec<Occurrence>>);

impl<K: Ord> OccurrenceIndex<K> {
    pub(super) fn push(&mut self, key: K, event: FactId, span: Span) {
        self.0
            .entry(key)
            .or_default()
            .push(Occurrence { event, span });
    }

    pub(super) fn normalize(&mut self) {
        for occurrences in self.0.values_mut() {
            occurrences.sort_by_key(|occurrence| {
                (occurrence.event, occurrence.span.lo, occurrence.span.hi)
            });
            occurrences.dedup();
        }
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
