//! Internal project tables used while a project is being assembled.
//!
//! The wrappers centralize duplicate detection and preserve insertion order for
//! deterministic project traversal.

use std::collections::BTreeMap;

use crate::project::{
    ProjectInputError, ProjectRelativePath, ResolutionRequestKey, ResolverOutcome, SourceFile,
};

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
