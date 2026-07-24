//! Typed occurrence storage and deterministic normalization.
//!
//! Occurrences are sorted by semantic fact identity and source span, then
//! deduplicated within each key. Queries can therefore borrow stable slices
//! and emit evidence without repeating normalization policy.

use std::collections::{BTreeMap, BTreeSet};

use glass_lint_datastructures::{ByteRange, NameId, NamePath, SymbolPath};
use smol_str::SmolStr;

use crate::analysis::facts::FactId;

/// A borrowed, merged, or owned collection of candidate occurrences.
///
/// Exact indexed lookups borrow the normalized slice without allocation.
/// Merged lookups iterate two sorted slices without allocation. Scanned
/// lookups (package queries, predicate scans) still own a `Vec` because
/// they combine multiple index buckets.
pub(in crate::analysis) enum CandidateOccurrences<'a> {
    Indexed(&'a [Occurrence]),
    Borrowed(BorrowedOccurrenceIter<'a>),
    BorrowedPackage(BorrowedPackageOccurrenceIter<'a>),
    Scanned(Vec<Occurrence>),
}

/// Iterator over candidate occurrences from any lookup strategy.
pub(in crate::analysis) enum CandidateOccurrenceIter<'a> {
    Indexed(core::iter::Copied<core::slice::Iter<'a, Occurrence>>),
    Borrowed(BorrowedOccurrenceIter<'a>),
    BorrowedPackage(BorrowedPackageOccurrenceIter<'a>),
    Scanned(std::vec::IntoIter<Occurrence>),
}

impl Iterator for CandidateOccurrenceIter<'_> {
    type Item = Occurrence;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Indexed(iter) => iter.next(),
            Self::Borrowed(iter) => iter.next(),
            Self::BorrowedPackage(iter) => iter.next(),
            Self::Scanned(iter) => iter.next(),
        }
    }
}

impl<'a> IntoIterator for CandidateOccurrences<'a> {
    type IntoIter = CandidateOccurrenceIter<'a>;
    type Item = Occurrence;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::Indexed(slice) => CandidateOccurrenceIter::Indexed(slice.iter().copied()),
            Self::Borrowed(iter) => CandidateOccurrenceIter::Borrowed(iter),
            Self::BorrowedPackage(iter) => CandidateOccurrenceIter::BorrowedPackage(iter),
            Self::Scanned(vec) => CandidateOccurrenceIter::Scanned(vec.into_iter()),
        }
    }
}

/// Deterministically merges normalized occurrence slices without owning any
/// occurrence values. A `base` slice and borrowed `overlay` buckets are merged
/// without allocating a combined bucket vector.
#[derive(Clone, Debug)]
pub(in crate::analysis) struct BorrowedOccurrenceIter<'a> {
    base: Option<&'a [Occurrence]>,
    overlay: &'a [&'a [Occurrence]],
    positions: Vec<usize>,
}

impl<'a> BorrowedOccurrenceIter<'a> {
    pub(super) fn new(base: Option<&'a [Occurrence]>, overlay: &'a [&'a [Occurrence]]) -> Self {
        let num_buckets = overlay.len() + usize::from(base.is_some());
        Self {
            base,
            overlay,
            positions: vec![0; num_buckets],
        }
    }
}

impl Iterator for BorrowedOccurrenceIter<'_> {
    type Item = Occurrence;

    fn next(&mut self) -> Option<Self::Item> {
        let has_base = self.base.is_some();
        let next = (0..self.positions.len())
            .filter_map(|index| {
                let bucket = if has_base {
                    if index == 0 {
                        self.base?
                    } else {
                        self.overlay.get(index - 1)?
                    }
                } else {
                    self.overlay.get(index)?
                };
                let position = self.positions[index];
                bucket.get(position).map(|occurrence| {
                    (
                        occurrence.event,
                        occurrence.span.start(),
                        occurrence.span.end(),
                        index,
                        *occurrence,
                    )
                })
            })
            .min_by_key(|(event, start, end, index, _)| (*event, *start, *end, *index));
        let (_, _, _, chosen_index, occurrence) = next?;
        if has_base && chosen_index == 0 {
            if let Some(base) = self.base
                && self.positions[0] < base.len()
                && base[self.positions[0]] == occurrence
            {
                self.positions[0] += 1;
            }
        } else {
            let overlay_index = if has_base {
                chosen_index - 1
            } else {
                chosen_index
            };
            if let Some(bucket) = self.overlay.get(overlay_index)
                && self.positions[chosen_index] < bucket.len()
                && bucket[self.positions[chosen_index]] == occurrence
            {
                self.positions[chosen_index] += 1;
            }
        }
        Some(occurrence)
    }
}

/// Package-match predicate borrowed from a compiled query clause.
///
/// Package clauses match every module-export key whose module satisfies
/// the pattern and whose export equals the target. This predicate is
/// a concrete type so the lazy [`PackageOccurrenceIter`] can call it
/// without boxing a closure.
#[derive(Clone, Debug)]
pub(in crate::analysis) struct PackageKeyPredicate<'a> {
    pattern: &'a crate::api::rule::ModuleSpecifierPattern,
    kind: PackageMatchKind<'a>,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) enum PackageMatchKind<'a> {
    Export(&'a SmolStr),
    Namespace(&'a SymbolPath),
}

impl<'a> PackageKeyPredicate<'a> {
    pub(super) fn new(
        pattern: &'a crate::api::rule::ModuleSpecifierPattern,
        kind: PackageMatchKind<'a>,
    ) -> Self {
        Self { pattern, kind }
    }

    fn matches(&self, key: &ModuleExportKey) -> bool {
        if !self.pattern.matches(key.module()) {
            return false;
        }
        match self.kind {
            PackageMatchKind::Export(expected) => key.export() == expected,
            PackageMatchKind::Namespace(member) => member.eq_chain(key.export()),
        }
    }
}

/// Lazy package scan over owned base buckets and borrowed linked buckets.
#[derive(Clone, Debug)]
pub(in crate::analysis) struct BorrowedPackageOccurrenceIter<'a> {
    predicate: PackageKeyPredicate<'a>,
    masked: Option<&'a BTreeSet<ModuleExportKey>>,
    base_iter: std::collections::btree_map::Iter<'a, ModuleExportKey, Vec<Occurrence>>,
    overlay_iter:
        Option<std::collections::btree_map::Iter<'a, ModuleExportKey, Vec<&'a [Occurrence]>>>,
    current: Option<BorrowedOccurrenceIter<'a>>,
    checking_base: bool,
}

impl<'a> BorrowedPackageOccurrenceIter<'a> {
    pub(super) fn new(
        predicate: PackageKeyPredicate<'a>,
        masked: Option<&'a BTreeSet<ModuleExportKey>>,
        base: &'a BTreeMap<ModuleExportKey, Vec<Occurrence>>,
        overlay: Option<&'a BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>>>,
    ) -> Self {
        Self {
            predicate,
            masked,
            base_iter: base.iter(),
            overlay_iter: overlay.map(BTreeMap::iter),
            current: None,
            checking_base: true,
        }
    }
}

impl Iterator for BorrowedPackageOccurrenceIter<'_> {
    type Item = Occurrence;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = &mut self.current
                && let Some(occurrence) = current.next()
            {
                return Some(occurrence);
            }
            self.current = None;

            if self.checking_base {
                if let Some((key, values)) = self.base_iter.next() {
                    if self.predicate.matches(key)
                        && self.masked.is_none_or(|mask| !mask.contains(key))
                    {
                        self.current =
                            Some(BorrowedOccurrenceIter::new(Some(values.as_slice()), &[]));
                    }
                    continue;
                }
                self.checking_base = false;
            }

            let Some(iter) = &mut self.overlay_iter else {
                return None;
            };
            let Some((key, values)) = iter.next() else {
                self.overlay_iter = None;
                return None;
            };
            if self.predicate.matches(key) {
                self.current = Some(BorrowedOccurrenceIter::new(None, values.as_slice()));
            }
        }
    }
}

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
    /// Access the underlying map for lazy package-scan iteration.
    pub(super) fn as_map(&self) -> &BTreeMap<K, Vec<Occurrence>> {
        &self.0
    }

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
    /// predicate.
    pub(super) fn matching(
        &self,
        mut predicate: impl FnMut(&K) -> bool,
    ) -> Option<CandidateOccurrences<'_>> {
        let occurrences = self
            .0
            .iter()
            .filter(|(key, _)| predicate(key))
            .flat_map(|(_, values)| values.iter().copied())
            .collect::<Vec<_>>();
        if occurrences.is_empty() {
            return None;
        }
        Some(CandidateOccurrences::Scanned(occurrences))
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

    pub(in crate::analysis) fn into_parts(self) -> (SmolStr, SmolStr) {
        (self.module, self.export)
    }

    pub(in crate::analysis) fn wildcard(module: impl Into<SmolStr>) -> Self {
        Self::new(module, "*")
    }
}

pub(in crate::analysis) type ModuleOccurrences = OccurrenceIndex<ModuleExportKey>;
