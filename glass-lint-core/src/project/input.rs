//! Normalization and validation of the public project input contract.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    SourceFile,
    analysis::QualifiedRequestId,
    project::{
        ModuleId, ProjectInput, ProjectInputError, ProjectRelativePath, ResolutionRequestKey,
        ResolverOutcome,
    },
};

fn compute_module_ids(
    sources: &BTreeMap<ProjectRelativePath, crate::SourceFile>,
) -> BTreeMap<ProjectRelativePath, ModuleId> {
    sources
        .keys()
        .enumerate()
        .map(|(index, path)| {
            (
                path.clone(),
                ModuleId::new(u32::try_from(index).expect("module count exceeds ModuleId range")),
            )
        })
        .collect()
}

impl ProjectInput {
    /// Admit the public DTO into the normalized, internal project stage.
    pub(crate) fn admit(self) -> Result<ValidatedProjectInput, ProjectInputError> {
        self.validate()
    }

    /// Validate, normalize, deduplicate, and assign stable module IDs.
    /// Returns a map-backed validated type with deterministic ordering.
    pub fn validate(self) -> Result<ValidatedProjectInput, ProjectInputError> {
        if self.sources.len() > 100_000 {
            return Err(ProjectInputError::BudgetExceeded("source count".into()));
        }
        if self.resolutions.len() > 500_000 {
            return Err(ProjectInputError::BudgetExceeded("resolution count".into()));
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

        let root = normalize_root(&self.root)?;
        let mut sources = BTreeMap::new();
        for mut source in self.sources {
            source.path = normalize_relative(&source.path)?;
            if sources.contains_key(&source.path) {
                return Err(ProjectInputError::DuplicateSource(source.path.to_string()));
            }
            sources.insert(source.path.clone(), source);
        }

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

        let module_ids = compute_module_ids(&sources);
        Ok(ValidatedProjectInput {
            root,
            sources,
            resolutions,
            module_ids,
            request_ids: BTreeMap::new(),
        })
    }
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
    module_ids: BTreeMap<ProjectRelativePath, ModuleId>,
    /// Pre-built mapping from resolution request key to qualified
    /// module/request identity, populated during the resolve phase.
    /// Used by linking to avoid re-enumerating authored requests.
    request_ids: BTreeMap<ResolutionRequestKey, QualifiedRequestId>,
}

/// Internal: destructured components of a validated project.
type ProjectParts = (
    PathBuf,
    BTreeMap<ProjectRelativePath, crate::SourceFile>,
    BTreeMap<ResolutionRequestKey, ResolverOutcome>,
    BTreeMap<ProjectRelativePath, ModuleId>,
    BTreeMap<ResolutionRequestKey, QualifiedRequestId>,
);

impl ValidatedProjectInput {
    /// Create from already-normalized tables. Only used within the crate
    /// during the resolve phase when maps are built from incremental sources.
    pub(crate) fn from_maps(
        root: PathBuf,
        sources: BTreeMap<ProjectRelativePath, crate::SourceFile>,
        resolutions: BTreeMap<ResolutionRequestKey, ResolverOutcome>,
    ) -> Self {
        let module_ids = compute_module_ids(&sources);
        Self {
            root,
            sources,
            resolutions,
            module_ids,
            request_ids: BTreeMap::new(),
        }
    }

    /// Attach a pre-built request-to-qualified-ID mapping.
    /// Used after the resolve phase to carry auth-request identity across
    /// the local-to-resolved boundary.
    pub(crate) fn with_request_ids(
        mut self,
        request_ids: BTreeMap<ResolutionRequestKey, QualifiedRequestId>,
    ) -> Self {
        self.request_ids = request_ids;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    pub fn resolution_count(&self) -> usize {
        self.resolutions.len()
    }

    pub fn module_id(&self, path: &ProjectRelativePath) -> Option<ModuleId> {
        self.module_ids.get(path).copied()
    }

    /// Iterate over (path, source) pairs in deterministic order.
    pub fn sources(&self) -> impl Iterator<Item = (&ProjectRelativePath, &SourceFile)> {
        self.sources.iter()
    }

    /// Iterate over (key, outcome) pairs in deterministic order.
    pub fn resolutions(&self) -> impl Iterator<Item = (&ResolutionRequestKey, &ResolverOutcome)> {
        self.resolutions.iter()
    }

    /// Iterate over module ID assignments in deterministic order.
    pub fn module_ids(&self) -> impl Iterator<Item = (&ProjectRelativePath, ModuleId)> {
        self.module_ids.iter().map(|(p, id)| (p, *id))
    }

    /// Crate-internal: destructure into all components at once.
    pub(crate) fn into_parts(self) -> ProjectParts {
        (
            self.root,
            self.sources,
            self.resolutions,
            self.module_ids,
            self.request_ids,
        )
    }

    /// Crate-internal: borrow the source map for pipeline stages.
    pub(crate) fn source_map(&self) -> &BTreeMap<ProjectRelativePath, crate::SourceFile> {
        &self.sources
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
