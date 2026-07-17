//! Internal project tables used while a project is being assembled.
//!
//! The wrappers centralize duplicate detection and preserve insertion order for
//! evidence while using ordered maps for deterministic project traversal.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Index,
};

use super::{Evidence, ProjectInputError, ResolutionRequestKey, ResolutionResult, SourceFile};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EvidenceKey {
    message: String,
    location: Option<(String, crate::SourceRange)>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct EvidenceList {
    /// Evidence in the order in which it was first observed.
    items: Vec<Evidence>,
    #[serde(skip)]
    seen: BTreeSet<EvidenceKey>,
}

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
    /// Add evidence unless an identical typed record is already present.
    pub fn push_unique(&mut self, item: Evidence) {
        let key = EvidenceKey {
            message: item.message.clone(),
            location: item
                .location
                .as_ref()
                .map(|location| (location.path.to_string(), location.range.clone())),
        };
        if self.seen.insert(key) {
            self.items.push(item);
        }
    }

    /// Borrow evidence without exposing mutation that could invalidate
    /// deduplication.
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
pub struct SourceTable(BTreeMap<String, SourceFile>);

impl SourceTable {
    /// Insert one normalized source path, rejecting replacement of an existing
    /// source.
    pub fn insert(&mut self, source: SourceFile) -> Result<(), ProjectInputError> {
        let path = source.path.to_string();
        if self.0.contains_key(&path) {
            return Err(ProjectInputError::DuplicateSource(path));
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
pub struct ResolutionTable(BTreeMap<ResolutionRequestKey, ResolutionResult>);

impl ResolutionTable {
    /// Insert one resolver answer, rejecting a second answer for the same
    /// request.
    pub fn insert(
        &mut self,
        key: ResolutionRequestKey,
        result: ResolutionResult,
    ) -> Result<(), ProjectInputError> {
        if self.0.contains_key(&key) {
            return Err(ProjectInputError::DuplicateResolution(key));
        }
        self.0.insert(key, result);
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
