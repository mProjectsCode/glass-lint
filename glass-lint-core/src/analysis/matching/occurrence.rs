//! Typed occurrence storage and deterministic normalization.
//!
//! Occurrences are sorted by semantic fact identity and source span, then
//! deduplicated within each key. Queries can therefore borrow stable slices
//! and emit evidence without repeating normalization policy.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, binary_heap::BinaryHeap},
};

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
///
/// When only one bucket is present a zero-allocation cursor fast path is used.
/// For multiple buckets a binary-heap k-way merge avoids O(k) scans per item.
#[derive(Clone, Debug)]
pub(in crate::analysis) struct BorrowedOccurrenceIter<'a> {
    base: Option<&'a [Occurrence]>,
    overlay: &'a [&'a [Occurrence]],
    state: MergeState,
}

/// Internal k-way merge or single-cursor state.
#[derive(Clone, Debug)]
enum MergeState {
    /// Single bucket: cursor at (bucket_index, position_in_bucket).
    Cursor(usize, usize),
    /// Multiple buckets: min-heap with per-bucket positions.
    Multi {
        positions: Vec<usize>,
        heap: BinaryHeap<Reverse<MergeItem>>,
    },
}

/// One candidate occurrence tracked by the heap, ordered by
/// (event, start, end, bucket) for deterministic tie-breaking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MergeItem {
    event: FactId,
    start: u32,
    end: u32,
    bucket: usize,
    occurrence: Occurrence,
}

impl Ord for MergeItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.event, self.start, self.end, self.bucket).cmp(&(
            other.event,
            other.start,
            other.end,
            other.bucket,
        ))
    }
}

impl PartialOrd for MergeItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> BorrowedOccurrenceIter<'a> {
    pub(super) fn new(base: Option<&'a [Occurrence]>, overlay: &'a [&'a [Occurrence]]) -> Self {
        let num_buckets = overlay.len() + usize::from(base.is_some());
        let state = if num_buckets <= 1 {
            MergeState::Cursor(0, 0)
        } else {
            let mut heap = BinaryHeap::with_capacity(num_buckets);
            let positions = vec![0; num_buckets];
            for i in 0..num_buckets {
                let slice = bucket_slice(base, overlay, i);
                push_candidate(&mut heap, i, slice, 0);
            }
            MergeState::Multi { positions, heap }
        };
        Self {
            base,
            overlay,
            state,
        }
    }
}

/// Return the slice for a given logical bucket index, matching the
/// indexing convention used by [`BorrowedOccurrenceIter`]:
///
/// | has\_base | index 0 | index ≥ 1 |
/// |---|---|---|
/// | true  | base          | overlay\[index-1\] |
/// | false | overlay\[0\]  | overlay\[index\]   |
fn bucket_slice<'a>(
    base: Option<&'a [Occurrence]>,
    overlay: &'a [&'a [Occurrence]],
    index: usize,
) -> Option<&'a [Occurrence]> {
    match (base, index) {
        (Some(base), 0) => Some(base),
        (Some(_), n) => overlay.get(n - 1).copied(),
        (None, n) => overlay.get(n).copied(),
    }
}

/// Push the element at `position` from `slice` onto the heap as a candidate.
fn push_candidate(
    heap: &mut BinaryHeap<Reverse<MergeItem>>,
    bucket: usize,
    slice: Option<&[Occurrence]>,
    position: usize,
) {
    let Some(slice) = slice else { return };
    if let Some(&occurrence) = slice.get(position) {
        heap.push(Reverse(MergeItem {
            event: occurrence.event,
            start: occurrence.span.start(),
            end: occurrence.span.end(),
            bucket,
            occurrence,
        }));
    }
}

impl Iterator for BorrowedOccurrenceIter<'_> {
    type Item = Occurrence;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            MergeState::Cursor(bucket, pos) => {
                let slice = bucket_slice(self.base, self.overlay, *bucket)?;
                let occurrence = *slice.get(*pos)?;
                *pos += 1;
                Some(occurrence)
            }
            MergeState::Multi { positions, heap } => {
                let Reverse(item) = heap.pop()?;
                let bucket = item.bucket;
                let slice = bucket_slice(self.base, self.overlay, bucket);
                positions[bucket] += 1;
                push_candidate(heap, bucket, slice, positions[bucket]);
                Some(item.occurrence)
            }
        }
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

    /// Deduplicate every key bucket.
    ///
    /// Entries are already appended in monotonically increasing `(event, span)`
    /// order because `build_from_stream` iterates facts in FactId order and
    /// all pushes within a bucket for the same fact are sequential.
    pub(super) fn normalize(&mut self) {
        for occurrences in self.0.values_mut() {
            occurrences.dedup_by_key(|occurrence| {
                (
                    occurrence.event,
                    occurrence.span.start(),
                    occurrence.span.end(),
                )
            });
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
