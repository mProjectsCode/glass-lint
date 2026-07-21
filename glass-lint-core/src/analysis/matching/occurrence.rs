//! Typed occurrence storage and deterministic normalization.
//!
//! Occurrences are sorted by semantic fact identity and source span, then
//! deduplicated within each key. Queries can therefore borrow stable slices
//! and emit evidence without repeating normalization policy.

use std::collections::BTreeMap;

use smol_str::SmolStr;

use crate::{
    ByteRange,
    analysis::{facts::FactId, name::NameId, value::NamePath},
};

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
    /// Look up one normalized occurrence bucket as a slice.
    pub(super) fn get<Q>(&self, key: &Q) -> Option<&[Occurrence]>
    where
        K: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.0.get(key).map(Vec::as_slice)
    }

    /// Whether no occurrence buckets are present.
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over keys and normalized occurrence buckets.
    pub(super) fn iter(&self) -> impl Iterator<Item = (&K, &[Occurrence])> {
        self.0.iter().map(|(k, v)| (k, v.as_slice()))
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

pub(in crate::analysis) type Occurrences = OccurrenceIndex<SmolStr>;
pub(in crate::analysis) type NameOccurrences = OccurrenceIndex<NameId>;

/// Stable key for a module request and one exported member.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(in crate::analysis) struct ModuleExportKey {
    module: SmolStr,
    export: SmolStr,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(in crate::analysis) struct InstanceMemberKey {
    identity: ModuleExportKey,
    member: SmolStr,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(in crate::analysis) struct ReturnedMemberKey {
    source: NamePath,
    member: NamePath,
}

impl ReturnedMemberKey {
    pub(in crate::analysis) fn new(source: NamePath, member: NamePath) -> Self {
        Self { source, member }
    }

    pub(in crate::analysis) fn source(&self) -> &NamePath {
        &self.source
    }

    pub(in crate::analysis) fn member(&self) -> &NamePath {
        &self.member
    }
}

impl InstanceMemberKey {
    pub(in crate::analysis) fn new(
        module: impl Into<SmolStr>,
        export: impl Into<SmolStr>,
        member: impl Into<SmolStr>,
    ) -> Self {
        Self {
            identity: ModuleExportKey::new(module, export),
            member: member.into(),
        }
    }

    pub(in crate::analysis) fn identity(&self) -> &ModuleExportKey {
        &self.identity
    }

    pub(in crate::analysis) fn member(&self) -> &SmolStr {
        &self.member
    }
}

impl ModuleExportKey {
    pub(in crate::analysis) fn new(module: impl Into<SmolStr>, export: impl Into<SmolStr>) -> Self {
        Self {
            module: module.into(),
            export: export.into(),
        }
    }

    pub(in crate::analysis) fn module(&self) -> &SmolStr {
        &self.module
    }

    pub(in crate::analysis) fn export(&self) -> &SmolStr {
        &self.export
    }

    pub(in crate::analysis) fn wildcard(module: impl Into<SmolStr>) -> Self {
        Self::new(module, "*")
    }
}

/// Lazy merge of two sorted, deduplicated occurrence slices.
///
/// Both inputs must already be sorted by `(event, span.start(), span.end())`
/// and free of internal duplicates. The merge yields every element in global
/// order and skips duplicates that appear in both inputs.
#[derive(Debug, Clone)]
pub(in crate::analysis) struct MergeOccurrenceIter<'a> {
    left: &'a [Occurrence],
    right: &'a [Occurrence],
    left_pos: usize,
    right_pos: usize,
}

impl<'a> MergeOccurrenceIter<'a> {
    pub(super) fn new(left: &'a [Occurrence], right: &'a [Occurrence]) -> Self {
        Self {
            left,
            right,
            left_pos: 0,
            right_pos: 0,
        }
    }
}

impl Iterator for MergeOccurrenceIter<'_> {
    type Item = Occurrence;

    fn next(&mut self) -> Option<Self::Item> {
        let left_done = self.left_pos >= self.left.len();
        let right_done = self.right_pos >= self.right.len();
        match (left_done, right_done) {
            (true, true) => None,
            (true, false) => {
                let item = self.right[self.right_pos];
                self.right_pos += 1;
                Some(item)
            }
            (false, true) => {
                let item = self.left[self.left_pos];
                self.left_pos += 1;
                Some(item)
            }
            (false, false) => {
                let l = &self.left[self.left_pos];
                let r = &self.right[self.right_pos];
                let ordering = (l.event, l.span.start(), l.span.end()).cmp(&(
                    r.event,
                    r.span.start(),
                    r.span.end(),
                ));
                match ordering {
                    std::cmp::Ordering::Less => {
                        self.left_pos += 1;
                        Some(*l)
                    }
                    std::cmp::Ordering::Greater => {
                        self.right_pos += 1;
                        Some(*r)
                    }
                    std::cmp::Ordering::Equal => {
                        self.left_pos += 1;
                        self.right_pos += 1;
                        Some(*l)
                    }
                }
            }
        }
    }
}

pub(in crate::analysis) type ModuleOccurrences = OccurrenceIndex<ModuleExportKey>;
