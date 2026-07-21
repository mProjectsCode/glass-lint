//! Normalization and validation of the public project input contract.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use crate::project::{
    ModuleId, ProjectInput, ProjectInputError, ProjectRelativePath, ResolutionRequestKey,
    ResolverOutcome,
};

type ValidatedMaps = (
    PathBuf,
    BTreeMap<ProjectRelativePath, crate::SourceFile>,
    BTreeMap<ResolutionRequestKey, ResolverOutcome>,
);

fn compute_module_ids(
    sources: &BTreeMap<ProjectRelativePath, crate::SourceFile>,
) -> BTreeMap<ProjectRelativePath, ModuleId> {
    sources
        .keys()
        .enumerate()
        .map(|(index, path)| {
            (
                path.clone(),
                ModuleId::new(
                    u32::try_from(index).expect("module count exceeds ModuleId range"),
                ),
            )
        })
        .collect()
}

impl ProjectInput {
    /// Admit the public DTO into the normalized, internal project stage.
    ///
    /// Validates and stores the result as authoritative typed maps, avoiding
    /// the map-to-Vec-to-map conversion that the public `validate` path
    /// still performs for external consumers.
    pub(crate) fn admit(self) -> Result<ValidatedProjectInput, ProjectInputError> {
        let (root, sources, resolutions) = self.validate_into_maps()?;
        let module_ids = compute_module_ids(&sources);
        Ok(ValidatedProjectInput {
            root,
            sources,
            resolutions,
            module_ids,
        })
    }

    /// Canonicalizes project identities and validates all cross-record
    /// references. Returns a public DTO compatible with external callers.
    pub fn validate(self) -> Result<Self, ProjectInputError> {
        let mut input = self;
        input.validate_budgets()?;
        input.root = normalize_root(&input.root)?;
        let sources_map = input.validate_sources()?;
        let source_paths: BTreeSet<_> = sources_map.keys().cloned().collect();
        input.validate_resolutions(&source_paths)?;
        // Convert maps back to Vecs for the public DTO contract.
        input.sources = sources_map.into_values().collect();
        input.resolutions = input.resolutions.into_iter().collect();
        Ok(input)
    }

    /// Shared budget checks.
    fn validate_budgets(&self) -> Result<(), ProjectInputError> {
        if self.sources.len() > 100_000 {
            return Err(ProjectInputError::BudgetExceeded("source count".into()));
        }
        if self.resolutions.len() > 500_000 {
            return Err(ProjectInputError::BudgetExceeded(
                "resolution count".into(),
            ));
        }
        if self
            .sources
            .iter()
            .map(|source| source.source.len())
            .sum::<usize>()
            > 512 * 1024 * 1024
        {
            return Err(ProjectInputError::BudgetExceeded(
                "project source bytes".into(),
            ));
        }
        Ok(())
    }

    /// Validate and deduplicate sources, returning the canonical map.
    fn validate_sources(&mut self) -> Result<BTreeMap<ProjectRelativePath, crate::SourceFile>, ProjectInputError> {
        let mut sources = BTreeMap::new();
        for mut source in std::mem::take(&mut self.sources) {
            source.path = normalize_relative(&source.path)?;
            let path = source.path.clone();
            if sources.insert(path.clone(), source).is_some() {
                return Err(ProjectInputError::DuplicateSource(path.to_string()));
            }
        }
        Ok(sources)
    }

    /// Validate and deduplicate resolutions using the canonical source map.
    fn validate_resolutions(
        &mut self,
        source_paths: &BTreeSet<ProjectRelativePath>,
    ) -> Result<(), ProjectInputError> {
        let mut resolutions = BTreeMap::new();
        for (mut key, mut result) in std::mem::take(&mut self.resolutions) {
            normalize_resolution_key(&mut key)?;
            if !source_paths.contains(&key.importer) {
                return Err(ProjectInputError::UnknownImporter(
                    key.importer.to_string(),
                ));
            }
            normalize_result(&mut result)?;
            if resolutions.insert(key.clone(), result).is_some() {
                return Err(ProjectInputError::DuplicateResolution(key));
            }
        }
        self.resolutions = resolutions.into_iter().collect();
        Ok(())
    }

    /// Validate and return maps directly, skipping the Vec round-trip.
    fn validate_into_maps(self) -> Result<ValidatedMaps, ProjectInputError> {
        self.validate_budgets()?;
        let root = normalize_root(&self.root)?;
        let mut sources = BTreeMap::new();
        for mut source in self.sources {
            source.path = normalize_relative(&source.path)?;
            let path = source.path.clone();
            if sources.insert(path.clone(), source).is_some() {
                return Err(ProjectInputError::DuplicateSource(path.to_string()));
            }
        }
        let source_paths: BTreeSet<_> = sources.keys().cloned().collect();
        let mut resolutions = BTreeMap::new();
        for (mut key, mut result) in self.resolutions {
            normalize_resolution_key(&mut key)?;
            if !source_paths.contains(&key.importer) {
                return Err(ProjectInputError::UnknownImporter(
                    key.importer.to_string(),
                ));
            }
            normalize_result(&mut result)?;
            if resolutions.insert(key.clone(), result).is_some() {
                return Err(ProjectInputError::DuplicateResolution(key));
            }
        }
        Ok((root, sources, resolutions))
    }

    /// Assign stable IDs from normalized path order. Test-only because the
    /// canonical IDs are precomputed during [`ProjectInput::admit`].
    #[cfg(test)]
    #[must_use]
    pub fn module_ids(&self) -> BTreeMap<ProjectRelativePath, ModuleId> {
        let mut paths = self
            .sources
            .iter()
            .map(|source| source.path.clone())
            .collect::<Vec<_>>();
        paths.sort();
        paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                (
                    path,
                    ModuleId::new(
                        u32::try_from(index).expect("module count exceeds ModuleId range"),
                    ),
                )
            })
            .collect()
    }
}

/// Project records after path, target, duplicate, and cross-reference
/// validation. This type is crate-private by design: public callers continue
/// to exchange `ProjectInput`, while canonical analysis stages cannot
/// accidentally re-run admission validation.
///
/// Sources and resolutions are stored as stable-ordered maps so callers
/// receive authoritative, indexed representations without map-to-vector
/// conversion or re-indexing.
#[derive(Debug)]
pub(crate) struct ValidatedProjectInput {
    pub(crate) root: PathBuf,
    pub(crate) sources: BTreeMap<ProjectRelativePath, crate::SourceFile>,
    pub(crate) resolutions: BTreeMap<ResolutionRequestKey, ResolverOutcome>,
    pub(crate) module_ids: BTreeMap<ProjectRelativePath, ModuleId>,
}

/// Validate the root path that anchors project-relative normalization.
pub fn normalize_root(path: &Path) -> Result<PathBuf, ProjectInputError> {
    if path.as_os_str().is_empty() {
        Err(ProjectInputError::InvalidPath(String::new()))
    } else {
        Ok(path.to_path_buf())
    }
}

/// Normalize a project-relative path and reject escapes/absolute paths.
pub fn normalize_relative(path: impl AsRef<str>) -> Result<ProjectRelativePath, ProjectInputError> {
    let original = path.as_ref().to_string();
    let path = path.as_ref().replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path.split('/').any(|part| part == "..")
    {
        return Err(ProjectInputError::InvalidPath(original));
    }
    let parts = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        Err(ProjectInputError::InvalidPath(original))
    } else {
        Ok(ProjectRelativePath::from_normalized(parts.join("/")))
    }
}

/// Normalize an explicitly outside-project target without losing absoluteness.
pub fn normalize_outside_target(path: &str) -> Result<String, ProjectInputError> {
    let original = path.to_string();
    let path = path.replace('\\', "/");
    if path.is_empty() || path.contains('\0') {
        return Err(ProjectInputError::InvalidPath(original));
    }
    let absolute = path.starts_with('/');
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if absolute {
                continue;
            }
            if parts.last().is_some_and(|last| *last != "..") {
                parts.pop();
            } else {
                parts.push(part);
            }
        } else {
            parts.push(part);
        }
    }
    if parts.is_empty() {
        return Err(ProjectInputError::InvalidPath(original));
    }
    Ok(if absolute {
        format!("/{}", parts.join("/"))
    } else {
        parts.join("/")
    })
}

/// Normalize and validate one typed resolver result.
pub fn normalize_result(result: &mut ResolverOutcome) -> Result<(), ProjectInputError> {
    match result {
        ResolverOutcome::Internal { path } => *path = normalize_relative(path.as_str())?,
        ResolverOutcome::OutsideProject { path } => *path = normalize_outside_target(path)?,
        ResolverOutcome::External { package } if package.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(package.clone()));
        }
        ResolverOutcome::Builtin { name } if name.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(name.clone()));
        }
        ResolverOutcome::Unsupported { reason } if reason.trim().is_empty() => {
            return Err(ProjectInputError::InvalidTarget(reason.clone()));
        }
        _ => {}
    }
    Ok(())
}

/// Normalize an importer/range key and enforce one-based ordered positions.
pub fn normalize_resolution_key(key: &mut ResolutionRequestKey) -> Result<(), ProjectInputError> {
    key.importer = normalize_relative(key.importer.as_str())?;
    Ok(())
}
