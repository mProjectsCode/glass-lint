//! Internal project tables used while a project is being assembled.

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Deref, DerefMut},
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

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EvidenceList {
    items: Vec<ProjectEvidence>,
    #[serde(skip)]
    seen: BTreeSet<EvidenceKey>,
}

impl EvidenceList {
    pub fn push_unique(&mut self, item: ProjectEvidence) {
        let key = EvidenceKey {
            message: item.message.clone(),
            location: item
                .location
                .as_ref()
                .map(|location| (location.path.clone(), location.range.clone())),
            source: item.source.clone(),
        };
        if self.seen.insert(key) {
            self.items.push(item);
        }
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
impl Deref for EvidenceList {
    type Target = Vec<ProjectEvidence>;

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}
impl DerefMut for EvidenceList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.items
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
    pub fn insert(&mut self, source: SourceFile) -> Result<(), ProjectInputError> {
        let path = source.path.clone();
        if self.0.insert(path.clone(), source).is_some() {
            return Err(ProjectInputError::DuplicateSource(path));
        }
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<&SourceFile> {
        self.0.get(path)
    }

    pub fn into_values(self) -> impl Iterator<Item = SourceFile> {
        self.0.into_values()
    }
}

#[derive(Debug, Default)]
pub struct ResolutionTable(BTreeMap<ResolutionRequestKey, ResolutionResult>);

impl ResolutionTable {
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
            location: None,
            source: Some("a".into()),
        };
        let second = ProjectEvidence {
            message: "path".into(),
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
