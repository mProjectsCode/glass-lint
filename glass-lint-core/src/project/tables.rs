//! Internal project tables used while a project is being assembled.
//!
//! The wrappers centralize duplicate detection and preserve normalized path
//! order for deterministic project traversal.

use std::collections::BTreeMap;

use crate::project::{
    ModuleId, ProjectInputError, ProjectPhaseError, ProjectRelativePath, ResolutionRequestKey,
    ResolverOutcome, SourceFile,
};

#[derive(Debug, Default)]
pub struct SourceTable {
    sources: BTreeMap<ProjectRelativePath, SourceFile>,
    source_bytes: usize,
}

impl SourceTable {
    /// Insert one normalized source path into an empty staging table,
    /// rejecting replacement of an existing source.
    fn insert(&mut self, source: SourceFile) -> Result<(), ProjectInputError> {
        let path = source.path().clone();
        if self.sources.contains_key(&path) {
            return Err(ProjectInputError::DuplicateSource(path.to_string()));
        }
        self.source_bytes = self.source_bytes.checked_add(source.source().len()).ok_or(
            ProjectInputError::SourceBytesExceeded {
                limit: usize::MAX,
                attempted: usize::MAX,
            },
        )?;
        self.sources.insert(path, source);
        Ok(())
    }

    /// Admit a bounded batch atomically, rejecting duplicates and limit
    /// violations before changing either table.
    pub fn admit_all(
        &mut self,
        sources: impl IntoIterator<Item = SourceFile>,
        max_sources: usize,
        max_source_bytes: usize,
    ) -> Result<(), ProjectInputError> {
        let mut staged = Self::default();
        for source in sources {
            let path = source.path().clone();
            if self.sources.contains_key(&path) {
                return Err(ProjectInputError::DuplicateSource(path.to_string()));
            }
            staged.insert(source)?;
            let attempted_sources = self.len().checked_add(staged.len()).ok_or(
                ProjectInputError::SourceCountExceeded {
                    limit: max_sources,
                    attempted: usize::MAX,
                },
            )?;
            if attempted_sources > max_sources {
                return Err(ProjectInputError::SourceCountExceeded {
                    limit: max_sources,
                    attempted: attempted_sources,
                });
            }
            let attempted_bytes = self.source_bytes.checked_add(staged.source_bytes).ok_or(
                ProjectInputError::SourceBytesExceeded {
                    limit: max_source_bytes,
                    attempted: usize::MAX,
                },
            )?;
            if attempted_bytes > max_source_bytes {
                return Err(ProjectInputError::SourceBytesExceeded {
                    limit: max_source_bytes,
                    attempted: attempted_bytes,
                });
            }
        }
        self.source_bytes = self.source_bytes.checked_add(staged.source_bytes).ok_or(
            ProjectInputError::SourceBytesExceeded {
                limit: max_source_bytes,
                attempted: usize::MAX,
            },
        )?;
        self.sources.append(&mut staged.sources);
        Ok(())
    }

    pub(crate) fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn get(&self, path: &ProjectRelativePath) -> Option<&SourceFile> {
        self.sources.get(path)
    }

    /// Iterate sources in normalized project-path order.
    pub(crate) fn in_normalized_path_order(
        &self,
    ) -> impl Iterator<Item = (&ProjectRelativePath, &SourceFile)> {
        self.sources.iter()
    }

    /// Assign module IDs in normalized project-path order.
    pub(crate) fn module_ids(
        &self,
    ) -> Result<BTreeMap<ProjectRelativePath, ModuleId>, ProjectPhaseError> {
        self.sources
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
