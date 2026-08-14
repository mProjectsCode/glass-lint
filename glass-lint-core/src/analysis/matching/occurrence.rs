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

/// A raw borrowed, merged, or owned occurrence selection.
///
/// Selection is intentionally lazy and preserves duplicate physical events.
/// Call [`Self::into_ordered`] at the evidence boundary to establish one
/// deterministic order without changing the count represented by the raw
/// selection. Normalized indexed and merged selections retain their lazy
/// iterators; only concatenated selections are materialized and sorted.
pub(in crate::analysis) enum OccurrenceSelection<'a> {
    Indexed(core::iter::Copied<core::slice::Iter<'a, Occurrence>>),
    Borrowed(BorrowedOccurrenceIter<'a>),
    BorrowedPackage(BorrowedPackageOccurrenceIter<'a>),
    Scanned(ScannedOccurrences),
}

pub(in crate::analysis) struct ScannedOccurrences {
    values: Vec<Occurrence>,
    next: usize,
}

impl<'a> OccurrenceSelection<'a> {
    pub(super) fn indexed(slice: &'a [Occurrence]) -> Self {
        Self::Indexed(slice.iter().copied())
    }

    pub(super) fn scanned(occurrences: Vec<Occurrence>) -> Self {
        Self::Scanned(ScannedOccurrences {
            values: occurrences,
            next: 0,
        })
    }

    /// Convert candidates to the common evidence order while retaining
    /// duplicates for the evidence count and later presentation policy.
    pub(super) fn into_ordered(self) -> OrderedOccurrences<'a> {
        match self {
            Self::Indexed(iter) => OrderedOccurrences::Indexed(iter),
            Self::Borrowed(iter) => OrderedOccurrences::Borrowed(iter),
            Self::BorrowedPackage(iter) => OrderedOccurrences::sorted(iter),
            Self::Scanned(scanned) => OrderedOccurrences::sorted(scanned.values),
        }
    }
}

pub(super) enum OrderedOccurrences<'a> {
    Indexed(core::iter::Copied<core::slice::Iter<'a, Occurrence>>),
    Borrowed(BorrowedOccurrenceIter<'a>),
    Sorted(std::vec::IntoIter<Occurrence>),
}

impl OrderedOccurrences<'_> {
    fn sorted(occurrences: impl IntoIterator<Item = Occurrence>) -> Self {
        let mut occurrences = occurrences.into_iter().collect::<Vec<_>>();
        occurrences.sort_unstable_by_key(|occurrence| {
            (
                occurrence.event,
                occurrence.span.start(),
                occurrence.span.end(),
            )
        });
        Self::Sorted(occurrences.into_iter())
    }
}

impl Iterator for OrderedOccurrences<'_> {
    type Item = Occurrence;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Indexed(iter) => iter.next(),
            Self::Borrowed(iter) => iter.next(),
            Self::Sorted(iter) => iter.next(),
        }
    }
}

impl Iterator for OccurrenceSelection<'_> {
    type Item = Occurrence;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Indexed(iter) => iter.next(),
            Self::Borrowed(iter) => iter.next(),
            Self::BorrowedPackage(iter) => iter.next(),
            Self::Scanned(scanned) => {
                let value = scanned.values.get(scanned.next).copied();
                scanned.next = scanned.next.saturating_add(1);
                value
            }
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

/// Linked package buckets with their masking policy.
///
/// The package iterator consumes this semantic overlay instead of receiving
/// its masking set and nested bucket map as separate storage-shaped inputs.
#[derive(Clone, Copy, Debug)]
pub(in crate::analysis) struct PackageOverlay<'a> {
    masked: &'a BTreeSet<ModuleExportKey>,
    buckets: &'a BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>>,
}

impl<'a> PackageOverlay<'a> {
    pub(super) fn new(
        masked: &'a BTreeSet<ModuleExportKey>,
        buckets: &'a BTreeMap<ModuleExportKey, Vec<&'a [Occurrence]>>,
    ) -> Self {
        Self { masked, buckets }
    }
}

impl<'a> BorrowedPackageOccurrenceIter<'a> {
    pub(super) fn base(
        predicate: PackageKeyPredicate<'a>,
        base: &'a BTreeMap<ModuleExportKey, Vec<Occurrence>>,
    ) -> Self {
        Self::new(predicate, base, None)
    }

    pub(super) fn with_overlay(
        predicate: PackageKeyPredicate<'a>,
        base: &'a BTreeMap<ModuleExportKey, Vec<Occurrence>>,
        overlay: PackageOverlay<'a>,
    ) -> Self {
        Self::new(predicate, base, Some(overlay))
    }

    fn new(
        predicate: PackageKeyPredicate<'a>,
        base: &'a BTreeMap<ModuleExportKey, Vec<Occurrence>>,
        overlay: Option<PackageOverlay<'a>>,
    ) -> Self {
        let masked = overlay.as_ref().map(|overlay| overlay.masked);
        let overlay_iter = overlay.map(|overlay| overlay.buckets.iter());
        Self {
            predicate,
            masked,
            base_iter: base.iter(),
            overlay_iter,
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
    /// Canonical source span used for trace correlation and tie-breaking.
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

    /// Iterate over buckets for reference assertions in unit tests.
    #[cfg(test)]
    pub(super) fn iter(&self) -> impl Iterator<Item = (&K, &[Occurrence])> {
        self.0.iter().map(|(key, values)| (key, values.as_slice()))
    }

    /// Collect occurrences from all buckets satisfying one identity
    /// predicate.
    pub(super) fn matching(
        &self,
        mut predicate: impl FnMut(&K) -> bool,
    ) -> Option<OccurrenceSelection<'_>> {
        let occurrences = self
            .0
            .iter()
            .filter(|(key, _)| predicate(key))
            .flat_map(|(_, values)| values.iter().copied())
            .collect::<Vec<_>>();
        if occurrences.is_empty() {
            return None;
        }
        Some(OccurrenceSelection::scanned(occurrences))
    }

    /// Append an already constructed occurrence before normalization.
    pub(super) fn push_occurrence(&mut self, key: K, occurrence: Occurrence) {
        self.0.entry(key).or_default().push(occurrence);
    }

    /// Append one event/span pair before normalization.
    #[cfg(test)]
    pub(super) fn push(&mut self, key: K, event: FactId, span: ByteRange) {
        self.push_occurrence(key, Occurrence::new(event, span));
    }

    /// Sort and deduplicate every key bucket.
    ///
    /// Sorting here makes the normalized ordering an owner invariant rather
    /// than a promise that every collector happened to append monotonically.
    pub(super) fn normalize(&mut self) {
        for occurrences in self.0.values_mut() {
            occurrences.sort_unstable_by_key(|occurrence| {
                (
                    occurrence.event,
                    occurrence.span.start(),
                    occurrence.span.end(),
                )
            });
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

impl OccurrenceIndex<ModuleExportKey> {
    /// Visit normalized module buckets without exposing the backing map.
    pub(super) fn for_each_bucket<'a>(
        &'a self,
        mut visit: impl FnMut(&ModuleExportKey, &'a [Occurrence]),
    ) {
        for (key, occurrences) in &self.0 {
            visit(key, occurrences.as_slice());
        }
    }

    /// Lazily scan package exports in the local occurrence index.
    pub(super) fn package_candidates<'a>(
        &'a self,
        predicate: PackageKeyPredicate<'a>,
    ) -> BorrowedPackageOccurrenceIter<'a> {
        BorrowedPackageOccurrenceIter::base(predicate, &self.0)
    }

    /// Lazily scan package exports with a completed linked overlay.
    pub(super) fn package_candidates_with_overlay<'a>(
        &'a self,
        predicate: PackageKeyPredicate<'a>,
        overlay: PackageOverlay<'a>,
    ) -> BorrowedPackageOccurrenceIter<'a> {
        BorrowedPackageOccurrenceIter::with_overlay(predicate, &self.0, overlay)
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

pub(in crate::analysis) type ModuleOccurrences = OccurrenceIndex<ModuleExportKey>;

#[cfg(test)]
mod tests {
    use glass_lint_datastructures::ByteRange;

    use super::*;

    fn span(start: u32, end: u32) -> ByteRange {
        ByteRange::new(start, end).unwrap()
    }

    fn occ(event_id: u32, start: u32, end: u32) -> Occurrence {
        Occurrence::new(FactId::from_test(event_id), span(start, end))
    }

    /// Reference merge: collect all occurrences then sort by
    /// (event, start, end, bucket) — mirrors the old O(k·n) contract.
    fn reference_merge<'a>(
        base: Option<&'a [Occurrence]>,
        overlay: &'a [&'a [Occurrence]],
    ) -> Vec<Occurrence> {
        let mut all: Vec<(Occurrence, usize)> = Vec::new();
        if let Some(b) = base {
            for &o in b {
                all.push((o, 0));
            }
        }
        for (bi, &bucket) in overlay.iter().enumerate() {
            let bucket_idx = usize::from(base.is_some()) + bi;
            for &o in bucket {
                all.push((o, bucket_idx));
            }
        }
        all.sort_by_key(|&(o, bi)| (o.event, o.span.start(), o.span.end(), bi));
        all.into_iter().map(|(o, _)| o).collect()
    }

    #[test]
    fn cursor_single_overlay_bucket() {
        let bucket = [occ(1, 5, 10), occ(2, 20, 30)];
        let overlays: Vec<&[Occurrence]> = vec![&bucket];
        let iter = BorrowedOccurrenceIter::new(None, &overlays);
        let result: Vec<_> = iter.collect();
        assert_eq!(result, vec![occ(1, 5, 10), occ(2, 20, 30)]);
    }

    #[test]
    fn cursor_base_only() {
        let base = [occ(1, 5, 10), occ(2, 20, 30)];
        let iter = BorrowedOccurrenceIter::new(Some(&base), &[]);
        let result: Vec<_> = iter.collect();
        assert_eq!(result, vec![occ(1, 5, 10), occ(2, 20, 30)]);
    }

    #[test]
    fn cursor_empty_overlay() {
        let mut iter = BorrowedOccurrenceIter::new(None, &[]);
        assert!(iter.next().is_none());
    }

    #[test]
    fn multi_merge_two_buckets() {
        let b0 = [occ(1, 5, 10), occ(3, 15, 20)];
        let b1 = [occ(2, 8, 12)];
        let overlays: Vec<&[Occurrence]> = vec![&b0, &b1];
        let iter = BorrowedOccurrenceIter::new(None, &overlays);
        let result: Vec<_> = iter.collect();
        let expected = reference_merge(None, &overlays);
        assert_eq!(result, expected);
    }

    #[test]
    fn multi_merge_three_buckets_interleaved() {
        let b0 = [occ(1, 10, 20), occ(4, 40, 50)];
        let b1 = [occ(2, 10, 20), occ(3, 30, 40)];
        let b2 = [occ(1, 5, 10), occ(5, 50, 60)];
        let overlays: Vec<&[Occurrence]> = vec![&b0, &b1, &b2];
        let iter = BorrowedOccurrenceIter::new(None, &overlays);
        let result: Vec<_> = iter.collect();
        let expected = reference_merge(None, &overlays);
        assert_eq!(result, expected);
    }

    #[test]
    fn multi_merge_base_and_overlay() {
        let base = [occ(1, 5, 10)];
        let overlay = [occ(2, 8, 12)];
        let overlays: Vec<&[Occurrence]> = vec![&overlay];
        let iter = BorrowedOccurrenceIter::new(Some(&base), &overlays);
        let result: Vec<_> = iter.collect();
        let expected = reference_merge(Some(&base), &overlays);
        assert_eq!(result, expected);
    }

    #[test]
    fn multi_merge_tie_break_by_bucket() {
        let b0 = [occ(1, 10, 20)];
        let b1 = [occ(1, 10, 20)];
        let overlays: Vec<&[Occurrence]> = vec![&b0, &b1];
        let iter = BorrowedOccurrenceIter::new(None, &overlays);
        let result: Vec<_> = iter.collect();
        let expected = reference_merge(None, &overlays);
        assert_eq!(result, expected);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn multi_merge_with_empty_buckets() {
        let b0: [Occurrence; 0] = [];
        let b1 = [occ(1, 5, 10), occ(3, 25, 30)];
        let b2: [Occurrence; 0] = [];
        let b3 = [occ(2, 8, 12)];
        let overlays: Vec<&[Occurrence]> = vec![&b0, &b1, &b2, &b3];
        let iter = BorrowedOccurrenceIter::new(None, &overlays);
        let result: Vec<_> = iter.collect();
        let expected = reference_merge(None, &overlays);
        assert_eq!(result, expected);
    }

    #[test]
    fn ordered_selection_sorts_without_deduplicating_physical_events() {
        let selection = OccurrenceSelection::scanned(vec![
            occ(3, 30, 31),
            occ(1, 10, 11),
            occ(1, 10, 11),
            occ(2, 20, 21),
        ]);

        let ordered: Vec<_> = selection.into_ordered().collect();
        assert_eq!(
            ordered,
            vec![
                occ(1, 10, 11),
                occ(1, 10, 11),
                occ(2, 20, 21),
                occ(3, 30, 31)
            ]
        );
    }

    #[test]
    fn ordered_normalized_selections_keep_their_lazy_order() {
        let values = [occ(1, 10, 11), occ(2, 20, 21)];
        let indexed: Vec<_> = OccurrenceSelection::indexed(&values)
            .into_ordered()
            .collect();
        assert_eq!(indexed, values);

        let borrowed = OccurrenceSelection::Borrowed(BorrowedOccurrenceIter::new(
            Some(&values),
            &[],
        ));
        let borrowed: Vec<_> = borrowed.into_ordered().collect();
        assert_eq!(borrowed, values);
    }

    #[test]
    fn multi_merge_base_is_empty() {
        let base: [Occurrence; 0] = [];
        let o0 = [occ(1, 5, 10), occ(2, 20, 30)];
        let o1 = [occ(3, 15, 20)];
        let overlays: Vec<&[Occurrence]> = vec![&o0, &o1];
        let iter = BorrowedOccurrenceIter::new(Some(&base), &overlays);
        let result: Vec<_> = iter.collect();
        let expected = reference_merge(Some(&base), &overlays);
        assert_eq!(result, expected);
    }

    #[test]
    fn multi_merge_large_bucket_set() {
        let buckets: Vec<Vec<Occurrence>> = (0u32..20)
            .map(|i| {
                let event = (i % 7) + 1;
                let start = (i * 3) % 50;
                let end = start + (i % 10) + 5;
                vec![occ(event, start, end)]
            })
            .collect();
        let overlays: Vec<&[Occurrence]> = buckets.iter().map(Vec::as_slice).collect();
        let iter = BorrowedOccurrenceIter::new(None, &overlays);
        let result: Vec<_> = iter.collect();
        let expected = reference_merge(None, &overlays);
        assert_eq!(result, expected);
    }

    #[test]
    fn multi_merge_base_and_multiple_overlays() {
        let base = [occ(2, 10, 20), occ(5, 50, 60)];
        let o0 = [occ(1, 5, 10), occ(3, 15, 20)];
        let o1 = [occ(4, 30, 40)];
        let overlays: Vec<&[Occurrence]> = vec![&o0, &o1];
        let iter = BorrowedOccurrenceIter::new(Some(&base), &overlays);
        let result: Vec<_> = iter.collect();
        let expected = reference_merge(Some(&base), &overlays);
        assert_eq!(result, expected);
    }

    #[test]
    fn multi_merge_preserves_duplicates() {
        let b0 = [occ(1, 5, 10), occ(2, 20, 30)];
        let b1 = [occ(1, 5, 10), occ(2, 20, 30)];
        let overlays: Vec<&[Occurrence]> = vec![&b0, &b1];
        let iter = BorrowedOccurrenceIter::new(None, &overlays);
        let result: Vec<_> = iter.collect();
        assert_eq!(result.len(), 4);
        assert_eq!(result[0], occ(1, 5, 10));
        assert_eq!(result[1], occ(1, 5, 10));
        assert_eq!(result[2], occ(2, 20, 30));
        assert_eq!(result[3], occ(2, 20, 30));
    }
}
