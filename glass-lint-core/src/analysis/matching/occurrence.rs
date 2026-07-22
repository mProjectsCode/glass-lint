//! Typed occurrence storage and deterministic normalization.
//!
//! Occurrences are sorted by semantic fact identity and source span, then
//! deduplicated within each key. Queries can therefore borrow stable slices
//! and emit evidence without repeating normalization policy.

use std::collections::{BTreeMap, BTreeSet};

use smol_str::SmolStr;

use crate::{
    ByteRange,
    analysis::{SymbolPath, facts::FactId, name::NameId, value::NamePath},
};

/// A borrowed, merged, or owned collection of candidate occurrences.
///
/// Exact indexed lookups borrow the normalized slice without allocation.
/// Merged lookups iterate two sorted slices without allocation. Scanned
/// lookups (package queries, predicate scans) still own a `Vec` because
/// they combine multiple index buckets.
pub(in crate::analysis) enum CandidateOccurrences<'a> {
    Indexed(&'a [Occurrence]),
    Merged(MergeOccurrenceIter<'a>),
    Package(PackageOccurrenceIter<'a>),
    Scanned(Vec<Occurrence>),
}

/// Iterator over candidate occurrences from any lookup strategy.
pub(in crate::analysis) enum CandidateOccurrenceIter<'a> {
    Indexed(core::iter::Copied<core::slice::Iter<'a, Occurrence>>),
    Merged(MergeOccurrenceIter<'a>),
    Package(PackageOccurrenceIter<'a>),
    Scanned(std::vec::IntoIter<Occurrence>),
}

impl Iterator for CandidateOccurrenceIter<'_> {
    type Item = Occurrence;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Indexed(iter) => iter.next(),
            Self::Merged(iter) => iter.next(),
            Self::Package(iter) => iter.next(),
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
            Self::Merged(iter) => CandidateOccurrenceIter::Merged(iter),
            Self::Package(iter) => CandidateOccurrenceIter::Package(iter),
            Self::Scanned(vec) => CandidateOccurrenceIter::Scanned(vec.into_iter()),
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

/// Lazy occurrence iterator for package-clause scans.
///
/// Scans the base index keys (filtered by the predicate and optional mask),
/// then scans overlay keys. Both maps are iterated in `BTreeMap` order so
/// output is deterministic. No intermediate `Vec<Occurrence>` is allocated.
#[derive(Clone)]
pub(in crate::analysis) struct PackageOccurrenceIter<'a> {
    predicate: PackageKeyPredicate<'a>,
    masked: Option<&'a BTreeSet<ModuleExportKey>>,
    map_iter: Option<std::collections::btree_map::Iter<'a, ModuleExportKey, Vec<Occurrence>>>,
    overlay: Option<&'a BTreeMap<ModuleExportKey, Vec<Occurrence>>>,
    vals: Option<&'a [Occurrence]>,
    pos: usize,
    checking_mask: bool,
    done: bool,
}

impl<'a> PackageOccurrenceIter<'a> {
    pub(super) fn new(
        predicate: PackageKeyPredicate<'a>,
        masked: Option<&'a BTreeSet<ModuleExportKey>>,
        base: &'a BTreeMap<ModuleExportKey, Vec<Occurrence>>,
        overlay: Option<&'a BTreeMap<ModuleExportKey, Vec<Occurrence>>>,
    ) -> Self {
        Self {
            predicate,
            masked,
            map_iter: Some(base.iter()),
            overlay,
            vals: None,
            pos: 0,
            checking_mask: true,
            done: false,
        }
    }
}

impl Iterator for PackageOccurrenceIter<'_> {
    type Item = Occurrence;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.done {
                return None;
            }
            if let Some(vals) = self.vals
                && self.pos < vals.len()
            {
                let occ = vals[self.pos];
                self.pos += 1;
                return Some(occ);
            }
            self.vals = None;
            self.pos = 0;

            if let Some(iter) = &mut self.map_iter
                && let Some((key, vals)) = iter.next()
            {
                if self.predicate.matches(key)
                    && (!self.checking_mask || self.masked.is_none_or(|m| !m.contains(key)))
                {
                    self.vals = Some(vals.as_slice());
                    continue;
                }
                continue;
            }
            self.map_iter = None;

            if self.checking_mask {
                self.checking_mask = false;
                if let Some(overlay) = self.overlay {
                    self.map_iter = Some(overlay.iter());
                    continue;
                }
            }
            self.done = true;
            return None;
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
