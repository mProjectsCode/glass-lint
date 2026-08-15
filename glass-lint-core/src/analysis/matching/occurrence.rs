//! Typed occurrence storage and deterministic normalization.
//!
//! Occurrences are sorted by semantic fact identity and source span, then
//! deduplicated within each key. Queries can therefore borrow stable slices
//! and emit evidence without repeating normalization policy.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, binary_heap::BinaryHeap},
};

use glass_lint_datastructures::{NameId, NamePath, SymbolPath};
use smol_str::SmolStr;

mod storage;

pub(in crate::analysis) use storage::{Occurrence, OccurrenceIndex};

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
    Scanned(std::vec::IntoIter<Occurrence>),
}

impl<'a> OccurrenceSelection<'a> {
    pub(super) fn indexed(slice: &'a [Occurrence]) -> Self {
        Self::Indexed(slice.iter().copied())
    }

    pub(super) fn scanned(occurrences: Vec<Occurrence>) -> Self {
        Self::Scanned(occurrences.into_iter())
    }

    /// Convert candidates to the common evidence order while retaining
    /// duplicates for the evidence count and later presentation policy.
    pub(super) fn into_ordered(self) -> OrderedOccurrences<'a> {
        match self {
            Self::Indexed(iter) => OrderedOccurrences::Indexed(iter),
            Self::Borrowed(iter) => OrderedOccurrences::Borrowed(iter),
            Self::BorrowedPackage(iter) => OrderedOccurrences::sorted(iter),
            Self::Scanned(iter) => OrderedOccurrences::sorted(iter),
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
        occurrences.sort_unstable_by_key(Occurrence::sort_key);
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
            Self::Scanned(iter) => iter.next(),
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
    bucket: usize,
    occurrence: Occurrence,
}

impl Ord for MergeItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.occurrence.sort_key(), self.bucket).cmp(&(other.occurrence.sort_key(), other.bucket))
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
        heap.push(Reverse(MergeItem { bucket, occurrence }));
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
mod tests;
