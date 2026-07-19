//! Typed occurrence storage and deterministic normalization.
//!
//! Occurrences are sorted by semantic fact identity and source span, then
//! deduplicated within each key. Queries can therefore borrow stable slices
//! and emit evidence without repeating normalization policy.

use std::collections::BTreeMap;

use super::super::facts::FactId;
use crate::{ByteRange, analysis::SymbolPath};

/// Typed occurrence storage. Keeping insertion and normalization in one
/// container prevents semantic collectors from inventing subtly different
/// span ordering or duplicate policies for each provenance view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) struct Occurrence {
    /// Canonical semantic event identity.
    event: FactId,
    /// Source span used for evidence rendering and tie-breaking.
    span: ByteRange,
}

impl Occurrence {
    /// Construct one typed event/span occurrence.
    pub(super) fn new(event: FactId, span: ByteRange) -> Self {
        Self { event, span }
    }

    /// Return the canonical event identity.
    pub(super) fn event(&self) -> FactId {
        self.event
    }

    /// Return the source span associated with the event.
    pub(super) fn span(&self) -> ByteRange {
        self.span
    }
}

#[derive(Clone, Debug)]
/// Ordered occurrence buckets keyed by a typed semantic identity.
pub(in crate::analysis) struct OccurrenceIndex<K: Ord>(BTreeMap<K, Vec<Occurrence>>);

impl<K: Ord> Default for OccurrenceIndex<K> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Ord> OccurrenceIndex<K> {
    /// Look up one normalized occurrence bucket.
    pub(super) fn get<Q>(&self, key: &Q) -> Option<&Vec<Occurrence>>
    where
        K: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.0.get(key)
    }

    /// Whether no occurrence buckets are present.
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over keys and normalized occurrence buckets.
    pub(super) fn iter(&self) -> impl Iterator<Item = (&K, &Vec<Occurrence>)> {
        self.0.iter()
    }

    /// Collect occurrences from all buckets satisfying one identity
    /// predicate, returning no result when the predicate matches nothing.
    pub(super) fn matching(
        &self,
        mut predicate: impl FnMut(&K) -> bool,
    ) -> Option<Vec<Occurrence>> {
        let occurrences = self
            .0
            .iter()
            .filter(|(key, _)| predicate(key))
            .flat_map(|(_, values)| values.iter().copied())
            .collect::<Vec<_>>();
        (!occurrences.is_empty()).then_some(occurrences)
    }

    /// Append an already constructed occurrence before normalization.
    pub(super) fn push_occurrence(&mut self, key: K, occurrence: Occurrence) {
        self.0.entry(key).or_default().push(occurrence);
    }

    /// Append one event/span pair before normalization.
    pub(super) fn push(&mut self, key: K, event: FactId, span: ByteRange) {
        self.push_occurrence(key, Occurrence::new(event, span));
    }

    /// Sort and deduplicate every key bucket deterministically.
    pub(super) fn normalize(&mut self) {
        for occurrences in self.0.values_mut() {
            occurrences.sort_by_key(|occurrence| {
                (
                    occurrence.event,
                    occurrence.span.start(),
                    occurrence.span.end(),
                )
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

pub(in crate::analysis) type Occurrences = OccurrenceIndex<String>;

/// Stable key for a module request and one exported member.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(in crate::analysis) struct ModuleExportKey {
    module: String,
    export: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(in crate::analysis) struct InstanceMemberKey {
    identity: ModuleExportKey,
    member: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(in crate::analysis) struct ReturnedMemberKey {
    source: SymbolPath,
    member: SymbolPath,
}

impl ReturnedMemberKey {
    pub(in crate::analysis) fn new(source: SymbolPath, member: SymbolPath) -> Self {
        Self { source, member }
    }

    pub(in crate::analysis) fn source(&self) -> &SymbolPath {
        &self.source
    }

    pub(in crate::analysis) fn member(&self) -> &SymbolPath {
        &self.member
    }
}

impl InstanceMemberKey {
    pub(in crate::analysis) fn new(
        module: impl Into<String>,
        export: impl Into<String>,
        member: impl Into<String>,
    ) -> Self {
        Self {
            identity: ModuleExportKey::new(module, export),
            member: member.into(),
        }
    }

    pub(in crate::analysis) fn identity(&self) -> &ModuleExportKey {
        &self.identity
    }

    pub(in crate::analysis) fn member(&self) -> &str {
        &self.member
    }
}

impl ModuleExportKey {
    pub(in crate::analysis) fn new(module: impl Into<String>, export: impl Into<String>) -> Self {
        Self {
            module: module.into(),
            export: export.into(),
        }
    }

    pub(in crate::analysis) fn module(&self) -> &str {
        &self.module
    }

    pub(in crate::analysis) fn export(&self) -> &str {
        &self.export
    }

    pub(in crate::analysis) fn wildcard(module: impl Into<String>) -> Self {
        Self::new(module, "*")
    }
}

pub(in crate::analysis) type ModuleOccurrences = OccurrenceIndex<ModuleExportKey>;
