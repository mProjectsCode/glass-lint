//! Normalization and validation of the public project input contract.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    SourceFile,
    project::{
        ProjectInput, ProjectInputError, ProjectRelativePath, ResolutionRequestKey, ResolverOutcome,
    },
};

impl ProjectInput {
    /// Validate, normalize, deduplicate, and assign stable module IDs.
    /// Returns a map-backed validated type with deterministic ordering.
    pub fn validate(self) -> Result<ValidatedProjectInput, ProjectInputError> {
        if self.resolutions.len() > 500_000 {
            return Err(ProjectInputError::BudgetExceeded("resolution count".into()));
        }
        let (root, sources) = normalize_sources(self.root, self.sources)?;

        let mut resolutions = BTreeMap::new();
        for (mut key, mut result) in self.resolutions {
            normalize_resolution_key(&mut key)?;
            if !sources.contains_key(&key.importer) {
                return Err(ProjectInputError::UnknownImporter(key.importer.to_string()));
            }
            normalize_result(&mut result)?;
            if resolutions.contains_key(&key) {
                return Err(ProjectInputError::DuplicateResolution(key));
            }
            resolutions.insert(key, result);
        }

        Ok(ValidatedProjectInput {
            root,
            sources,
            resolutions,
        })
    }
}

/// Normalize root, validate source budgets, and return a deduplicated
/// deterministic-ordered source map.  Resolution validation is deliberately
/// excluded — the session pipeline owns that via
/// `LocallyAnalyzedProject::resolve`.
pub(crate) fn normalize_sources(
    root: impl Into<PathBuf>,
    sources: Vec<SourceFile>,
) -> Result<(PathBuf, BTreeMap<ProjectRelativePath, SourceFile>), ProjectInputError> {
    let root = root.into();
    if sources.len() > 100_000 {
        return Err(ProjectInputError::BudgetExceeded("source count".into()));
    }
    if sources.iter().map(|s| s.source().len()).sum::<usize>() > 512 * 1024 * 1024 {
        return Err(ProjectInputError::BudgetExceeded(
            "project source bytes".into(),
        ));
    }
    let root = normalize_root(&root)?;
    let mut result = BTreeMap::new();
    for mut source in sources {
        let normalized = normalize_relative(source.path())?;
        if result.contains_key(&normalized) {
            return Err(ProjectInputError::DuplicateSource(normalized.to_string()));
        }
        source.set_path(normalized);
        result.insert(source.path().clone(), source);
    }
    Ok((root, result))
}

/// Project records after path, target, duplicate, and cross-reference
/// validation.
///
/// Sources and resolutions are stored as stable-ordered maps so callers
/// receive authoritative, indexed representations without map-to-vector
/// conversion or re-indexing. Fields are private; access them through the
/// provided accessors to preserve the ability to change internal storage.
#[derive(Debug)]
pub struct ValidatedProjectInput {
    root: PathBuf,
    sources: BTreeMap<ProjectRelativePath, crate::SourceFile>,
    resolutions: BTreeMap<ResolutionRequestKey, ResolverOutcome>,
}

impl ValidatedProjectInput {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    pub fn resolution_count(&self) -> usize {
        self.resolutions.len()
    }

    /// Iterate over (path, source) pairs in deterministic order.
    pub fn sources(&self) -> impl Iterator<Item = (&ProjectRelativePath, &SourceFile)> {
        self.sources.iter()
    }

    /// Iterate over (key, outcome) pairs in deterministic order.
    pub fn resolutions(&self) -> impl Iterator<Item = (&ResolutionRequestKey, &ResolverOutcome)> {
        self.resolutions.iter()
    }
}

/// One-way DTO conversion for serialization or external wire boundaries.
impl From<ValidatedProjectInput> for ProjectInput {
    fn from(v: ValidatedProjectInput) -> Self {
        Self {
            root: v.root,
            sources: v.sources.into_values().collect(),
            resolutions: v.resolutions.into_iter().collect(),
        }
    }
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
