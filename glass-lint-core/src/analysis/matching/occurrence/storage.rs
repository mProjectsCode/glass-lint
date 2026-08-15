use std::collections::BTreeMap;

use glass_lint_datastructures::ByteRange;

use super::{
    BorrowedPackageOccurrenceIter, ModuleExportKey, OccurrenceSelection, PackageKeyPredicate,
    PackageOverlay,
};
use crate::analysis::facts::FactId;

/// Typed occurrence storage. Keeping insertion and normalization in one
/// container prevents semantic collectors from inventing subtly different
/// span ordering or duplicate policies for each provenance view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::analysis) struct Occurrence {
    event: FactId,
    span: ByteRange,
}

impl Occurrence {
    pub(in crate::analysis::matching) fn new(event: FactId, span: ByteRange) -> Self {
        Self { event, span }
    }

    pub(in crate::analysis::matching) fn event(&self) -> FactId {
        self.event
    }

    pub(in crate::analysis::matching) fn span(&self) -> ByteRange {
        self.span
    }

    /// Canonical deterministic ordering key, owned by [`Occurrence`] so
    /// sorting, deduplication, and merging can never diverge.
    pub(in crate::analysis::matching) fn sort_key(&self) -> (FactId, u32, u32) {
        (self.event, self.span.start(), self.span.end())
    }
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct OccurrenceIndex<K: Ord>(BTreeMap<K, Vec<Occurrence>>);

impl<K: Ord> Default for OccurrenceIndex<K> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Ord> OccurrenceIndex<K> {
    pub(in crate::analysis::matching) fn get<Q>(&self, key: &Q) -> Option<&[Occurrence]>
    where
        K: std::borrow::Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.0.get(key).map(Vec::as_slice)
    }

    #[cfg(test)]
    pub(in crate::analysis::matching) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[cfg(test)]
    pub(in crate::analysis::matching) fn iter(&self) -> impl Iterator<Item = (&K, &[Occurrence])> {
        self.0.iter().map(|(key, values)| (key, values.as_slice()))
    }

    pub(in crate::analysis::matching) fn matching(
        &self,
        mut predicate: impl FnMut(&K) -> bool,
    ) -> Option<OccurrenceSelection<'_>> {
        let occurrences = self
            .0
            .iter()
            .filter(|(key, _)| predicate(key))
            .flat_map(|(_, values)| values.iter().copied())
            .collect::<Vec<_>>();
        (!occurrences.is_empty()).then(|| OccurrenceSelection::scanned(occurrences))
    }

    pub(in crate::analysis::matching) fn push_occurrence(
        &mut self,
        key: K,
        occurrence: Occurrence,
    ) {
        self.0.entry(key).or_default().push(occurrence);
    }

    #[cfg(test)]
    pub(in crate::analysis::matching) fn push(&mut self, key: K, event: FactId, span: ByteRange) {
        self.push_occurrence(key, Occurrence::new(event, span));
    }

    pub(in crate::analysis::matching) fn normalize(&mut self) {
        for occurrences in self.0.values_mut() {
            occurrences.sort_unstable_by_key(Occurrence::sort_key);
            occurrences.dedup_by_key(|occurrence| occurrence.sort_key());
        }
    }
}

impl OccurrenceIndex<ModuleExportKey> {
    pub(in crate::analysis::matching) fn for_each_bucket<'a>(
        &'a self,
        mut visit: impl FnMut(&ModuleExportKey, &'a [Occurrence]),
    ) {
        for (key, occurrences) in &self.0 {
            visit(key, occurrences.as_slice());
        }
    }

    pub(in crate::analysis::matching) fn package_candidates<'a>(
        &'a self,
        predicate: PackageKeyPredicate<'a>,
    ) -> BorrowedPackageOccurrenceIter<'a> {
        BorrowedPackageOccurrenceIter::base(predicate, &self.0)
    }

    pub(in crate::analysis::matching) fn package_candidates_with_overlay<'a>(
        &'a self,
        predicate: PackageKeyPredicate<'a>,
        overlay: PackageOverlay<'a>,
    ) -> BorrowedPackageOccurrenceIter<'a> {
        BorrowedPackageOccurrenceIter::with_overlay(predicate, &self.0, overlay)
    }
}
