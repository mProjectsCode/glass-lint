//! Canonical project-root and filesystem admission boundary.
//!
//! Every accepted path must pass through one [`SourceAdmission`]; containment,
//! exclusion, extension-support, and canonicalization all have one
//! authoritative implementation here.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use glass_lint_core::{ProjectRelativePath, SourceFile};

use crate::{
    corpus::read_source_bytes, error::ProjectLoadError, options::ValidatedProjectLoadOptions,
};

/// File-count budget with an authoritative admit gate.
///
/// Ensures every file admission path checks the same limit arithmetic.
#[derive(Clone, Debug)]
pub struct FileBudget {
    limit: usize,
    count: usize,
}

impl FileBudget {
    pub fn new(limit: usize) -> Self {
        Self { limit, count: 0 }
    }

    pub fn try_admit(&mut self) -> Result<(), ProjectLoadError> {
        let next = self.count.saturating_add(1);
        if next > self.limit {
            return Err(ProjectLoadError::TooManyFiles(self.limit));
        }
        self.count = next;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn limit(&self) -> usize {
        self.limit
    }
}

/// A set of admitted source paths with a shared file-count budget.
///
/// Duplicate admit attempts do not consume the budget; only unique files are
/// counted toward the configured limit. Returns
/// [`ProjectLoadError::TooManyFiles`] when the set reaches its capacity, which
/// stops the caller's traversal.
#[derive(Clone, Debug)]
pub struct AdmissionSet {
    paths: BTreeSet<AdmittedSourcePath>,
    budget: FileBudget,
}

impl AdmissionSet {
    pub fn new(limit: usize) -> Self {
        Self {
            paths: BTreeSet::new(),
            budget: FileBudget::new(limit),
        }
    }

    pub fn admit(&mut self, path: &AdmittedSourcePath) -> Result<bool, ProjectLoadError> {
        if self.paths.contains(path) {
            return Ok(false);
        }
        self.budget.try_admit()?;
        self.paths.insert(path.clone());
        Ok(true)
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    #[allow(dead_code)]
    pub fn contains(&self, path: &AdmittedSourcePath) -> bool {
        self.paths.contains(path)
    }

    #[allow(dead_code)]
    pub fn limit(&self) -> usize {
        self.budget.limit()
    }

    pub fn into_path_bufs(self) -> Vec<PathBuf> {
        self.paths
            .into_iter()
            .map(AdmittedSourcePath::into_path_buf)
            .collect()
    }

    pub fn into_admitted_paths(self) -> Vec<AdmittedSourcePath> {
        self.paths.into_iter().collect()
    }

    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = &AdmittedSourcePath> {
        self.paths.iter()
    }
}

/// A path proven canonical by the filesystem admission boundary.
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
pub struct AdmittedSourcePath {
    canonical: CanonicalProjectPath,
    relative: ProjectRelativePath,
}

impl AsRef<Path> for AdmittedSourcePath {
    fn as_ref(&self) -> &Path {
        self.canonical.as_ref()
    }
}

impl AdmittedSourcePath {
    pub(crate) fn into_path_buf(self) -> PathBuf {
        self.canonical.into_path_buf()
    }

    /// The project-relative, slash-normalized path established during
    /// admission.
    pub fn relative(&self) -> &ProjectRelativePath {
        &self.relative
    }
}

/// Owns the canonical project root and source-file admission policy.
///
/// Construct one [`SourceAdmission`] per project; its canonical root is
/// resolved once and shared by discovery, resolution, and loading.
#[derive(Clone)]
pub struct SourceAdmission<'a> {
    canonical_root: PathBuf,
    options: &'a ValidatedProjectLoadOptions,
}

/// Result of applying the canonical project boundary to one filesystem path.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PathAdmission {
    Admitted(AdmittedSourcePath),
    Outside(CanonicalProjectPath),
    Excluded(CanonicalProjectPath),
    Unsupported(CanonicalProjectPath),
}

impl<'a> SourceAdmission<'a> {
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
    pub fn canonicalize(&self, path: &Path) -> Result<CanonicalProjectPath, ProjectLoadError> {
        realpath(path).map(CanonicalProjectPath)
    }

    /// Test lexical containment in the canonical project-root namespace.
    pub fn is_inside_root(&self, path: &Path) -> bool {
        path.strip_prefix(&self.canonical_root).is_ok()
    }

    /// Fail with [`ProjectLoadError::SelectionOutsideRoot`] when `path` lies
    /// outside the root.
    pub fn check_inside_root(&self, path: &Path) -> Result<(), ProjectLoadError> {
        if self.is_inside_root(path) {
            Ok(())
        } else {
            Err(ProjectLoadError::SelectionOutsideRoot {
                selection: path.to_path_buf(),
                root: self.canonical_root.clone(),
            })
        }
    }

    /// Canonicalize a path and apply containment, exclusion, and extension
    /// policy exactly once.
    pub(crate) fn classify(&self, path: &Path) -> Result<PathAdmission, ProjectLoadError> {
        let canonical = self.canonicalize(path)?;
        if !self.is_inside_root(canonical.as_ref()) {
            return Ok(PathAdmission::Outside(canonical));
        }
        if self.is_excluded(canonical.as_ref()) {
            return Ok(PathAdmission::Excluded(canonical));
        }
        if !self.supports(canonical.as_ref()) {
            return Ok(PathAdmission::Unsupported(canonical));
        }
        let relative = self.make_relative(canonical.as_ref())?;
        Ok(PathAdmission::Admitted(AdmittedSourcePath {
            canonical,
            relative,
        }))
    }

    /// Compute the project-relative path for a canonical, root-contained path.
    fn make_relative(&self, path: &Path) -> Result<ProjectRelativePath, ProjectLoadError> {
        let relative = path
            .strip_prefix(&self.canonical_root)
            .expect("path was already confirmed inside root")
            .to_string_lossy()
            .replace('\\', "/");
        ProjectRelativePath::new(&relative)
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

    /// Canonicalize, check containment and support, read, and produce a
    /// normalized [`SourceFile`] in one pass.
    ///
    /// This is the single entry-point for loading an unvalidated path.
    pub fn load_source_file(&self, path: &Path) -> Result<SourceFile, ProjectLoadError> {
        match self.classify(path)? {
            PathAdmission::Admitted(admitted) => self.load_admitted_source_file(&admitted),
            PathAdmission::Outside(path) => Err(ProjectLoadError::SelectionOutsideRoot {
                selection: path.into_path_buf(),
                root: self.canonical_root.clone(),
            }),
            PathAdmission::Excluded(path) | PathAdmission::Unsupported(path) => {
                Err(ProjectLoadError::UnsupportedSource(path.into_path_buf()))
            }
        }
    }

    /// Read a path returned by [`Self::admitted_path`] without repeating the
    /// boundary decision. Does not canonicalize, re-admit, or re-check the
    /// extension.
    pub(crate) fn load_admitted_source_file(
        &self,
        admitted: &AdmittedSourcePath,
    ) -> Result<SourceFile, ProjectLoadError> {
        let corpus_file = read_source_bytes(admitted.as_ref(), self.options.max_source_bytes())?;
        Ok(SourceFile::from_relative(
            admitted.relative().clone(),
            corpus_file.source,
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
