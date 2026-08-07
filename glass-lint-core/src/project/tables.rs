//! Internal project tables used while a project is being assembled.
//!
//! The wrappers centralize duplicate detection and preserve insertion order for
//! deterministic project traversal.

use std::collections::BTreeMap;

use crate::project::{
    ModuleId, ProjectInputError, ProjectPhaseError, ProjectRelativePath, ResolutionRequestKey,
    ResolverOutcome, SourceFile,
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

    pub(crate) fn in_path_order(
        &self,
    ) -> impl Iterator<Item = (&ProjectRelativePath, &SourceFile)> {
        self.0.iter()
    }

    pub(crate) fn module_ids(
        &self,
    ) -> Result<BTreeMap<ProjectRelativePath, ModuleId>, ProjectPhaseError> {
        self.0
            .keys()
            .enumerate()
            .map(|(index, path)| {
                let id = u32::try_from(index).map_err(|_| {
                    ProjectPhaseError::BudgetExceeded("module count exceeds ModuleId range".into())
                })?;
                Ok((path.clone(), ModuleId::new(id)))
            })
            .collect()
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
    ) -> Result<(), ProjectPhaseError> {
        if self.0.contains_key(&key) {
            return Err(ProjectPhaseError::DuplicateResolution(key));
        }
        self.0.insert(key, result);
        Ok(())
    }
}

impl IntoIterator for ResolutionTable {
    type IntoIter = std::collections::btree_map::IntoIter<ResolutionRequestKey, ResolverOutcome>;
    type Item = (ResolutionRequestKey, ResolverOutcome);

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
