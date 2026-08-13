//! Canonical project-root and filesystem acceptance boundary.
//!
//! Every accepted path must pass through one [`SourceBoundary`]; containment,
//! exclusion, extension-support, and canonicalization all have one
//! authoritative implementation here.

use std::{
    borrow::Cow,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use glass_lint_core::project::{ProjectRelativePath, SourceFile};

use crate::{
    error::ProjectLoadError, options::ValidatedProjectLoadOptions, source_collection::read_source,
};

/// File-count budget with an authoritative acceptance gate.
///
/// Ensures every file acceptance path checks the same limit arithmetic.
#[derive(Clone, Debug)]
pub struct FileBudget {
    limit: usize,
    count: usize,
}

impl FileBudget {
    pub fn new(limit: usize) -> Self {
        Self { limit, count: 0 }
    }

    pub fn try_add(&mut self) -> Result<(), ProjectLoadError> {
        let next = self.count.saturating_add(1);
        if next > self.limit {
            return Err(ProjectLoadError::TooManyFiles(self.limit));
        }
        self.count = next;
        Ok(())
    }
}

/// A set of accepted source paths with a shared file-count budget.
///
/// Duplicate additions do not consume the budget; only unique files are
/// counted toward the configured limit. Returns
/// [`ProjectLoadError::TooManyFiles`] when the set reaches its capacity, which
/// stops the caller's traversal.
#[derive(Clone, Debug)]
pub struct AcceptedPaths {
    paths: BTreeSet<AcceptedSourcePath>,
    budget: FileBudget,
}

impl AcceptedPaths {
    pub fn new(limit: usize) -> Self {
        Self {
            paths: BTreeSet::new(),
            budget: FileBudget::new(limit),
        }
    }

    pub fn accept(&mut self, path: &AcceptedSourcePath) -> Result<bool, ProjectLoadError> {
        if self.paths.contains(path) {
            return Ok(false);
        }
        self.budget.try_add()?;
        self.paths.insert(path.clone());
        Ok(true)
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn into_path_bufs(self) -> Vec<PathBuf> {
        self.paths
            .into_iter()
            .map(AcceptedSourcePath::into_path_buf)
            .collect()
    }

    pub fn into_accepted_paths(self) -> Vec<AcceptedSourcePath> {
        self.paths.into_iter().collect()
    }
}

/// A path proven canonical by the filesystem acceptance boundary.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CanonicalProjectPath(PathBuf);

impl CanonicalProjectPath {
    pub(crate) fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl AsRef<Path> for CanonicalProjectPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// A canonical path proven to be inside the project and supported by policy,
/// alongside its project-relative identity.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AcceptedSourcePath {
    canonical: CanonicalProjectPath,
    relative: ProjectRelativePath,
}

impl AsRef<Path> for AcceptedSourcePath {
    fn as_ref(&self) -> &Path {
        self.canonical.as_ref()
    }
}

impl AcceptedSourcePath {
    pub(crate) fn into_path_buf(self) -> PathBuf {
        self.canonical.into_path_buf()
    }

    /// The project-relative, slash-normalized path established during
    /// acceptance.
    pub fn relative(&self) -> &ProjectRelativePath {
        &self.relative
    }
}

/// Owns the canonical project root and source-file acceptance policy.
///
/// Construct one [`SourceBoundary`] per project; its canonical root is
/// resolved once and shared by discovery, resolution, and loading.
#[derive(Clone)]
pub struct SourceBoundary<'a> {
    canonical_root: PathBuf,
    options: &'a ValidatedProjectLoadOptions,
}

/// Result of applying the canonical project boundary to one filesystem path.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PathClassification {
    Accepted(AcceptedSourcePath),
    Outside(CanonicalProjectPath),
    Excluded(CanonicalProjectPath),
    Unsupported(CanonicalProjectPath),
}

impl<'a> SourceBoundary<'a> {
    /// Establish one canonical root before any file I/O.
    pub fn new(
        root: &Path,
        options: &'a ValidatedProjectLoadOptions,
    ) -> Result<Self, ProjectLoadError> {
        let canonical_root = realpath(root)?;
        Ok(Self {
            canonical_root,
            options,
        })
    }

    /// The canonical project root established at construction.
    pub fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }

    /// Borrow the loader policy used for every boundary check.
    pub fn options(&self) -> &ValidatedProjectLoadOptions {
        self.options
    }

    /// Resolve a path to its canonical form through the filesystem.
    pub fn canonicalize(path: &Path) -> Result<CanonicalProjectPath, ProjectLoadError> {
        realpath(path).map(CanonicalProjectPath)
    }

    /// Test lexical containment in the canonical project-root namespace.
    pub fn is_inside_root(&self, path: &Path) -> bool {
        path.strip_prefix(&self.canonical_root).is_ok()
    }

    /// Canonicalize a path and apply containment, exclusion, and extension
    /// policy exactly once.
    pub(crate) fn classify(&self, path: &Path) -> Result<PathClassification, ProjectLoadError> {
        let canonical = Self::canonicalize(path)?;
        if !self.is_inside_root(canonical.as_ref()) {
            return Ok(PathClassification::Outside(canonical));
        }
        if self.is_excluded(canonical.as_ref()) {
            return Ok(PathClassification::Excluded(canonical));
        }
        if !self.supports(canonical.as_ref()) {
            return Ok(PathClassification::Unsupported(canonical));
        }
        let relative = self.make_relative(canonical.as_ref())?;
        Ok(PathClassification::Accepted(AcceptedSourcePath {
            canonical,
            relative,
        }))
    }

    /// Compute the project-relative path for a canonical, root-contained path.
    fn make_relative(&self, path: &Path) -> Result<ProjectRelativePath, ProjectLoadError> {
        let relative = path
            .strip_prefix(&self.canonical_root)
            .expect("path was already confirmed inside root")
            .to_str()
            .ok_or_else(|| ProjectLoadError::UnsupportedSource(path.to_path_buf()))?;
        let normalized = if relative.contains('\\') {
            Cow::Owned(relative.replace('\\', "/"))
        } else {
            Cow::Borrowed(relative)
        };
        ProjectRelativePath::new(&normalized)
            .map_err(|_| ProjectLoadError::UnsupportedSource(path.to_path_buf()))
    }

    /// Test whether a file extension is supported by the loader policy.
    pub fn supports(&self, path: &Path) -> bool {
        self.options.supports(path)
    }

    /// Test whether a path under the root has an excluded directory ancestor.
    pub fn is_excluded(&self, path: &Path) -> bool {
        self.options.excludes_path(&self.canonical_root, path)
    }

    /// Read a path returned by [`Self::classify`] as accepted without repeating
    /// the boundary decision. Does not canonicalize, re-accept, or re-check the
    /// extension.
    pub(crate) fn load_accepted_source_file(
        &self,
        accepted: &AcceptedSourcePath,
    ) -> Result<SourceFile, ProjectLoadError> {
        let source_file = read_source(accepted.as_ref(), self.options.max_source_bytes())?;
        let language = self
            .options
            .source_language(accepted.as_ref())
            .ok_or_else(|| ProjectLoadError::UnsupportedSource(accepted.as_ref().to_path_buf()))?;
        Ok(SourceFile::from_relative_with_language(
            accepted.relative().clone(),
            source_file.source,
            language,
        ))
    }
}

/// Canonicalize a path and preserve loader-specific I/O context on failure.
pub fn realpath(path: &Path) -> Result<PathBuf, ProjectLoadError> {
    fs::canonicalize(path).map_err(|source| ProjectLoadError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Make a selection path absolute without requiring it to exist on disk.
pub fn absolute_path(path: &Path) -> Result<PathBuf, ProjectLoadError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|source| ProjectLoadError::Io {
                path: PathBuf::from("."),
                source,
            })?
            .join(path))
    }
}
