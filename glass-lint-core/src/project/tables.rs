//! Internal project tables used while a project is being assembled.
//!
//! The wrappers centralize duplicate detection and preserve insertion order for
//! evidence while using ordered maps for deterministic project traversal.

use std::{collections::BTreeMap, ops::Index, sync::Arc};

use crate::project::{
    Evidence, ProjectInputError, ProjectRelativePath, ResolutionRequestKey, ResolverOutcome,
    SourceFile,
};

/// Identity-stable evidence collection with insertion-order preservation.
///
/// Duplicates are identified by message and location using a linear scan of
/// existing items. The list is typically small (<10 items), so the linear scan
/// avoids allocating a separate dedup-key store that would clone identity
/// fields from every pushed item.
///
/// Evidence may be owned directly or reference a shared immutable
/// `Arc<[Evidence]>` slice plus finding-specific local items. The shared
/// variant avoids cloning related evidence into every finding that shares it.
#[derive(Clone, Debug, Default)]
pub struct EvidenceList {
    /// Finding-specific evidence from local occurrences.
    local: Vec<Evidence>,
    /// Shared evidence (typically related cross-module evidence) owned once
    /// at rule-result scope.
    shared: Option<Arc<[Evidence]>>,
}

impl PartialEq for EvidenceList {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for EvidenceList {}

#[cfg(feature = "serde")]
impl serde::Serialize for EvidenceList {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for item in self {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
}

impl EvidenceList {
    /// Attach a shared evidence slice. Any previously set shared slice is
    /// replaced. Local evidence is preserved.
    pub fn set_shared(&mut self, shared: Arc<[Evidence]>) {
        self.shared = Some(shared);
    }

    /// Add evidence unless an identical record is already present.
    ///
    /// Identity is determined by message and location only, matching the
    /// deduplication semantics used during report assembly. A linear scan of
    /// existing items avoids cloning identity fields into a separate store.
    pub fn push_unique(&mut self, item: Evidence) {
        if !self.iter().any(|existing| {
            existing.message() == item.message() && existing.location() == item.location()
        }) {
            self.local.push(item);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.local.is_empty() && self.shared.as_ref().is_none_or(|s| s.is_empty())
    }

    pub fn len(&self) -> usize {
        self.local.len() + self.shared.as_ref().map_or(0, |s| s.len())
    }

    pub fn iter(&self) -> EvidenceIter<'_> {
        EvidenceIter {
            local: self.local.iter(),
            shared: self.shared.as_deref(),
            shared_index: 0,
        }
    }
}

pub struct EvidenceIter<'a> {
    local: std::slice::Iter<'a, Evidence>,
    shared: Option<&'a [Evidence]>,
    shared_index: usize,
}

impl<'a> Iterator for EvidenceIter<'a> {
    type Item = &'a Evidence;

    fn next(&mut self) -> Option<Self::Item> {
        self.local.next().or_else(|| {
            let item = self.shared?.get(self.shared_index);
            self.shared_index += 1;
            item
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.local.len()
            + self
                .shared
                .map_or(0, |s| s.len().saturating_sub(self.shared_index));
        (remaining, Some(remaining))
    }
}

impl FromIterator<Evidence> for EvidenceList {
    fn from_iter<T: IntoIterator<Item = Evidence>>(iter: T) -> Self {
        let mut list = Self::default();
        for item in iter {
            list.push_unique(item);
        }
        list
    }
}

impl Extend<Evidence> for EvidenceList {
    fn extend<T: IntoIterator<Item = Evidence>>(&mut self, iter: T) {
        for item in iter {
            self.push_unique(item);
        }
    }
}
impl Index<usize> for EvidenceList {
    type Output = Evidence;

    fn index(&self, index: usize) -> &Self::Output {
        let local_len = self.local.len();
        if index < local_len {
            &self.local[index]
        } else if let Some(shared) = &self.shared {
            &shared[index - local_len]
        } else {
            panic!("EvidenceList index out of bounds: {index}")
        }
    }
}
pub struct EvidenceIntoIter {
    local: std::vec::IntoIter<Evidence>,
    shared: Option<Arc<[Evidence]>>,
    shared_index: usize,
}

impl Iterator for EvidenceIntoIter {
    type Item = Evidence;

    fn next(&mut self) -> Option<Self::Item> {
        self.local.next().or_else(|| {
            let item = self.shared.as_ref()?.get(self.shared_index)?.clone();
            self.shared_index += 1;
            Some(item)
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.local.len()
            + self
                .shared
                .as_ref()
                .map_or(0, |s| s.len().saturating_sub(self.shared_index));
        (remaining, Some(remaining))
    }
}

impl IntoIterator for EvidenceList {
    type IntoIter = EvidenceIntoIter;
    type Item = Evidence;

    fn into_iter(self) -> Self::IntoIter {
        EvidenceIntoIter {
            local: self.local.into_iter(),
            shared: self.shared,
            shared_index: 0,
        }
    }
}
impl<'a> IntoIterator for &'a EvidenceList {
    type IntoIter = EvidenceIter<'a>;
    type Item = &'a Evidence;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::fmt::Display for EvidenceList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[derive(Debug, Default)]
pub struct SourceTable(BTreeMap<ProjectRelativePath, SourceFile>);

impl SourceTable {
    /// Insert one normalized source path, rejecting replacement of an existing
    /// source.
    pub fn insert(&mut self, source: SourceFile) -> Result<(), ProjectInputError> {
        let path = source.path().clone();
        if self.0.contains_key(&path) {
            return Err(ProjectInputError::DuplicateSource(path.to_string()));
        }
        self.0.insert(path, source);
        Ok(())
    }

    pub fn get(&self, path: &ProjectRelativePath) -> Option<&SourceFile> {
        self.0.get(path)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ProjectRelativePath, &SourceFile)> {
        self.0.iter()
    }

    pub(crate) fn into_map(self) -> BTreeMap<ProjectRelativePath, SourceFile> {
        self.0
    }
}

#[derive(Debug, Default)]
pub struct ResolutionTable(BTreeMap<ResolutionRequestKey, ResolverOutcome>);

impl ResolutionTable {
    /// Insert one resolver answer, rejecting a second answer for the same
    /// request.
    pub fn insert(
        &mut self,
        key: ResolutionRequestKey,
        result: ResolverOutcome,
    ) -> Result<(), ProjectInputError> {
        if self.0.contains_key(&key) {
            return Err(ProjectInputError::DuplicateResolution(key));
        }
        self.0.insert(key, result);
        Ok(())
    }

    pub(crate) fn into_map(self) -> BTreeMap<ResolutionRequestKey, ResolverOutcome> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_list_deduplicates_by_typed_identity_and_preserves_order() {
        let first = Evidence::new("path".into(), 1, false, None);
        let second = Evidence::new("other path".into(), 1, false, None);
        let duplicate = first.clone();
        let list = [first, second, duplicate]
            .into_iter()
            .collect::<EvidenceList>();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn shared_evidence_is_iterated_after_local() {
        let local: Vec<Evidence> = vec![Evidence::new("local".into(), 1, false, None)];
        let shared: Arc<[Evidence]> = vec![Evidence::new("shared".into(), 1, false, None)].into();
        let mut list: EvidenceList = local.into_iter().collect();
        list.set_shared(Arc::clone(&shared));

        assert_eq!(list.len(), 2);
        assert_eq!(list[0].message(), "local");
        assert_eq!(list[1].message(), "shared");
    }

    #[test]
    fn shared_evidence_has_same_owner_across_findings() {
        let shared: Arc<[Evidence]> = vec![Evidence::new("related".into(), 1, false, None)].into();
        let local_a: Vec<Evidence> = vec![Evidence::new("a".into(), 1, false, None)];
        let local_b: Vec<Evidence> = vec![Evidence::new("b".into(), 1, false, None)];

        let mut list_a: EvidenceList = local_a.into_iter().collect();
        list_a.set_shared(Arc::clone(&shared));
        let mut list_b: EvidenceList = local_b.into_iter().collect();
        list_b.set_shared(Arc::clone(&shared));

        assert_eq!(list_a.len(), 2);
        assert_eq!(list_b.len(), 2);
        assert_eq!(list_a[1].message(), "related");
        assert_eq!(list_b[1].message(), "related");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn shared_evidence_serializes_combined_with_local() {
        let shared: Arc<[Evidence]> = vec![Evidence::new("shared".into(), 1, false, None)].into();
        let mut list: EvidenceList = vec![Evidence::new("local".into(), 1, false, None)]
            .into_iter()
            .collect();
        list.set_shared(shared);

        let json = serde_json::to_value(&list).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 2);
        assert_eq!(json[0]["message"], "local");
        assert_eq!(json[1]["message"], "shared");
    }

    #[test]
    fn evidence_list_is_empty_only_when_both_sources_empty() {
        let mut list = EvidenceList::default();
        assert!(list.is_empty());

        list.set_shared(vec![Evidence::new("rel".into(), 1, false, None)].into());
        assert!(!list.is_empty());
    }

    #[test]
    fn push_unique_scans_both_local_and_shared() {
        let shared: Arc<[Evidence]> = vec![Evidence::new("shared".into(), 1, false, None)].into();
        let mut list: EvidenceList = vec![Evidence::new("local".into(), 1, false, None)]
            .into_iter()
            .collect();
        list.set_shared(shared);

        // Duplicate of shared should not be added
        list.push_unique(Evidence::new("shared".into(), 1, false, None));
        assert_eq!(list.len(), 2);

        // Novel item should be added to local
        list.push_unique(Evidence::new("new".into(), 1, false, None));
        assert_eq!(list.len(), 3);
    }
}
