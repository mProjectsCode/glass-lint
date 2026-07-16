//! Internal project tables used while a project is being assembled.
//!
//! The wrappers centralize duplicate detection and preserve insertion order for
//! evidence while using ordered maps for deterministic project traversal.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Index,
};

use super::{
    ProjectEvidence, ProjectInputError, ResolutionRequestKey, ResolutionResult, SourceFile,
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EvidenceKey {
    message: String,
    location: Option<(String, crate::SourceRange)>,
    source: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct EvidenceList {
    /// Evidence in the order in which it was first observed.
    items: Vec<ProjectEvidence>,
    #[serde(skip)]
    seen: BTreeSet<EvidenceKey>,
}

impl<'de> serde::Deserialize<'de> for EvidenceList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let items = Vec::<ProjectEvidence>::deserialize(deserializer)?;
        Ok(items.into_iter().collect())
    }
}

impl EvidenceList {
    /// Add evidence unless an identical typed record is already present.
    pub fn push_unique(&mut self, item: ProjectEvidence) {
        let key = EvidenceKey {
            message: item.message.clone(),
            location: item
                .location
                .as_ref()
                .map(|location| (location.path.to_string(), location.range.clone())),
            source: item.source.clone(),
        };
        if self.seen.insert(key) {
            self.items.push(item);
        }
    }

    /// Borrow evidence without exposing mutation that could invalidate
    /// deduplication.
    pub fn as_slice(&self) -> &[ProjectEvidence] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, ProjectEvidence> {
        self.items.iter()
    }
}

impl FromIterator<ProjectEvidence> for EvidenceList {
    fn from_iter<T: IntoIterator<Item = ProjectEvidence>>(iter: T) -> Self {
        let mut list = Self::default();
        for item in iter {
            list.push_unique(item);
        }
        list
    }
}

impl Extend<ProjectEvidence> for EvidenceList {
    fn extend<T: IntoIterator<Item = ProjectEvidence>>(&mut self, iter: T) {
        for item in iter {
            self.push_unique(item);
        }
    }
}
impl Index<usize> for EvidenceList {
    type Output = ProjectEvidence;

    fn index(&self, index: usize) -> &Self::Output {
        &self.items[index]
    }
}
impl IntoIterator for EvidenceList {
    type IntoIter = std::vec::IntoIter<ProjectEvidence>;
    type Item = ProjectEvidence;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}
impl<'a> IntoIterator for &'a EvidenceList {
    type IntoIter = std::slice::Iter<'a, ProjectEvidence>;
    type Item = &'a ProjectEvidence;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

#[derive(Debug, Default)]
pub struct SourceTable(BTreeMap<String, SourceFile>);

impl SourceTable {
    /// Insert one normalized source path, rejecting replacement of an existing
    /// source.
    pub fn insert(&mut self, source: SourceFile) -> Result<(), ProjectInputError> {
        let path = source.path.to_string();
        if self.0.insert(path.clone(), source).is_some() {
            return Err(ProjectInputError::DuplicateSource(path));
        }
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<&SourceFile> {
        self.0.get(path)
    }

    /// Consume the table in normalized path order.
    pub fn into_values(self) -> impl Iterator<Item = SourceFile> {
        self.0.into_values()
    }
}

#[derive(Debug, Default)]
pub struct ResolutionTable(BTreeMap<ResolutionRequestKey, ResolutionResult>);

impl ResolutionTable {
    /// Insert one resolver answer, rejecting a second answer for the same
    /// request.
    pub fn insert(
        &mut self,
        key: ResolutionRequestKey,
        result: ResolutionResult,
    ) -> Result<(), ProjectInputError> {
        if self.0.insert(key.clone(), result).is_some() {
            return Err(ProjectInputError::DuplicateResolution(key));
        }
        Ok(())
    }

    /// Consume the table in request-key order.
    pub fn into_values(self) -> impl Iterator<Item = (ResolutionRequestKey, ResolutionResult)> {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_list_deduplicates_by_typed_identity_and_preserves_order() {
        let first = ProjectEvidence {
            message: "path".into(),
            count: 1,
            evidence_truncated: false,
            location: None,
            source: Some("a".into()),
        };
        let second = ProjectEvidence {
            message: "path".into(),
            count: 1,
            evidence_truncated: false,
            location: None,
            source: Some("b".into()),
        };
        let duplicate = first.clone();
        let list = [first, second, duplicate]
            .into_iter()
            .collect::<EvidenceList>();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].source.as_deref(), Some("a"));
        assert_eq!(list[1].source.as_deref(), Some("b"));
    }
}
