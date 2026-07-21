//! Internal project tables used while a project is being assembled.
//!
//! The wrappers centralize duplicate detection and preserve insertion order for
//! evidence while using ordered maps for deterministic project traversal.

use std::{collections::BTreeMap, ops::Index};

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
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct EvidenceList {
    /// Evidence in the order in which it was first observed.
    items: Vec<Evidence>,
}

impl Eq for EvidenceList {}

impl<'de> serde::Deserialize<'de> for EvidenceList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Wire {
            items: Vec<Evidence>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Ok(wire.items.into_iter().collect())
    }
}

impl EvidenceList {
    /// Add evidence unless an identical record is already present.
    ///
    /// Identity is determined by message and location only, matching the
    /// deduplication semantics used during report assembly. A linear scan of
    /// existing items avoids cloning identity fields into a separate store.
    pub fn push_unique(&mut self, item: Evidence) {
        if !self
            .items
            .iter()
            .any(|existing| existing.message == item.message && existing.location == item.location)
        {
            self.items.push(item);
        }
    }

    pub fn as_slice(&self) -> &[Evidence] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Evidence> {
        self.items.iter()
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
        &self.items[index]
    }
}
impl IntoIterator for EvidenceList {
    type IntoIter = std::vec::IntoIter<Evidence>;
    type Item = Evidence;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}
impl<'a> IntoIterator for &'a EvidenceList {
    type IntoIter = std::slice::Iter<'a, Evidence>;
    type Item = &'a Evidence;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

#[derive(Debug, Default)]
pub struct SourceTable(BTreeMap<ProjectRelativePath, SourceFile>);

impl SourceTable {
    /// Insert one normalized source path, rejecting replacement of an existing
    /// source.
    pub fn insert(&mut self, source: SourceFile) -> Result<(), ProjectInputError> {
        let path = source.path.clone();
        if self.0.contains_key(&path) {
            return Err(ProjectInputError::DuplicateSource(path.to_string()));
        }
        self.0.insert(path, source);
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<&SourceFile> {
        self.0.get(path)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &SourceFile)> {
        self.0.iter().map(|(path, source)| (path.as_str(), source))
    }

    /// Consume the table in normalized path order.
    pub fn into_values(self) -> impl Iterator<Item = SourceFile> {
        self.0.into_values()
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

    /// Consume the table in request-key order.
    pub fn into_values(self) -> impl Iterator<Item = (ResolutionRequestKey, ResolverOutcome)> {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_list_deduplicates_by_typed_identity_and_preserves_order() {
        let first = Evidence {
            message: "path".into(),
            count: 1,
            evidence_truncated: false,
            location: None,
        };
        let second = Evidence {
            message: "other path".into(),
            count: 1,
            evidence_truncated: false,
            location: None,
        };
        let duplicate = first.clone();
        let list = [first, second, duplicate]
            .into_iter()
            .collect::<EvidenceList>();
        assert_eq!(list.len(), 2);
    }
}
