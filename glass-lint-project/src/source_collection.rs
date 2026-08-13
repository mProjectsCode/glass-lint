//! Bounded source collection and loading without project linking.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use crate::{
    boundary::{AcceptedPaths, PathClassification, SourceBoundary, realpath},
    budget::ProjectResourceBudget,
    error::ProjectLoadError,
    options::ValidatedProjectLoadOptions,
    walk,
};

#[derive(Clone, Debug)]
pub struct LoadedSource {
    /// Filesystem path retained for diagnostics and profiling.
    pub path: PathBuf,
    /// Byte length measured before decoding.
    pub bytes: u64,
    /// UTF-8 source text loaded under the configured byte limit.
    pub source: String,
}

/// Read source text from a trusted path with a byte budget.
///
/// This is the lowest-level read operation; callers must validate
/// extension support and containment before calling this function.
pub fn read_source(path: &Path, max_source_bytes: u64) -> Result<LoadedSource, ProjectLoadError> {
    let file = fs::File::open(path).map_err(|source| ProjectLoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let metadata = file.metadata().map_err(|source| ProjectLoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > max_source_bytes {
        return Err(ProjectLoadError::SourceTooLarge {
            path: path.to_path_buf(),
            bytes: metadata.len(),
            limit: max_source_bytes,
        });
    }
    let mut bytes = Vec::new();
    file.take(max_source_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| ProjectLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 > max_source_bytes {
        return Err(ProjectLoadError::SourceTooLarge {
            path: path.to_path_buf(),
            bytes: bytes.len() as u64,
            limit: max_source_bytes,
        });
    }
    let source = String::from_utf8(bytes).map_err(|error| ProjectLoadError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, error),
    })?;
    Ok(LoadedSource {
        path: path.to_path_buf(),
        bytes: source.len() as u64,
        source,
    })
}

/// A bounded source collection with one canonical project root.
///
/// Construction establishes one canonical source-acceptance boundary from
/// either the configured policy root or the first discovery root. That same
/// boundary is reused for every discovery and load operation within the
/// collection, so the containment boundary, exclusion rules, and extension
/// policy are applied consistently across all files.
pub struct SourceCollection {
    options: ValidatedProjectLoadOptions,
    canonical_root: Option<PathBuf>,
}

impl SourceCollection {
    /// Create a source collection from a policy already checked at the loader
    /// boundary.
    ///
    /// If the policy contains a configured root, it is canonicalized once and
    /// stored as the single project boundary for this collection. Without a
    /// configured root, the boundary is derived from each caller-supplied
    /// discovery root.
    pub fn from_validated(options: &ValidatedProjectLoadOptions) -> Result<Self, ProjectLoadError> {
        let canonical_root = match options.root() {
            Some(root) => Some(realpath(root)?),
            None => None,
        };
        Ok(Self {
            options: options.clone(),
            canonical_root,
        })
    }

    /// Create a source collection with an explicit canonical project root.
    ///
    /// The root is canonicalized immediately; discovery roots passed to
    /// [`Self::discover_filtered`] must be inside this root or equal to it.
    pub fn with_root(
        options: &ValidatedProjectLoadOptions,
        root: &Path,
    ) -> Result<Self, ProjectLoadError> {
        let canonical_root = realpath(root)?;
        Ok(Self {
            options: options.clone(),
            canonical_root: Some(canonical_root),
        })
    }

    /// Build or borrow the source boundary for this collection.
    ///
    /// When a canonical root was established at construction, uses it for every
    /// call. Otherwise derives the authority from `fallback_root` (backward
    /// compatible with callers that provide a per-call root).
    fn boundary(&self, fallback_root: &Path) -> Result<SourceBoundary<'_>, ProjectLoadError> {
        let root = self.canonical_root.as_deref().unwrap_or(fallback_root);
        SourceBoundary::new(root, &self.options)
    }

    /// The canonical project root if one was established at construction.
    pub fn root(&self) -> Option<&Path> {
        self.canonical_root.as_deref()
    }

    /// Discover supported files in deterministic path order.
    pub fn discover(&self, roots: &[PathBuf]) -> Result<Vec<PathBuf>, ProjectLoadError> {
        self.discover_filtered(roots, |_| true)
    }

    /// Discover files while applying a caller-owned membership predicate.
    ///
    /// When a canonical root was established at construction, every root in
    /// `roots` must be inside or equal to that root.
    pub fn discover_filtered(
        &self,
        roots: &[PathBuf],
        mut include: impl FnMut(&Path) -> bool,
    ) -> Result<Vec<PathBuf>, ProjectLoadError> {
        let mut accepted = AcceptedPaths::new(self.options.max_files());
        let mut budget = ProjectResourceBudget::new(
            self.options.max_visited_entries(),
            self.options.max_project_source_bytes(),
        );
        for root in roots {
            if let Some(canonical_root) = &self.canonical_root
                && root != canonical_root
                && root.strip_prefix(canonical_root).is_err()
            {
                return Err(ProjectLoadError::SelectionOutsideRoot {
                    selection: root.clone(),
                    root: canonical_root.clone(),
                });
            }
            let Some(metadata) = walk::resolve_root(&self.options, root)? else {
                continue;
            };
            let boundary = self.boundary(root)?;
            if metadata.is_file() {
                if include(root)
                    && let PathClassification::Accepted(path) = boundary.classify(root)?
                {
                    accepted.accept(&path)?;
                }
                continue;
            }
            if !metadata.is_dir() {
                return Err(ProjectLoadError::SourceRootNotFileOrDir(root.clone()));
            }
            walk::collect_files(
                &boundary,
                root,
                None,
                &mut include,
                &mut accepted,
                &mut budget,
            )?;
        }
        if accepted.len() > self.options.max_files() {
            return Err(ProjectLoadError::TooManyFiles(self.options.max_files()));
        }
        Ok(accepted.into_path_bufs())
    }

    /// Read one supported source file after enforcing the byte budget.
    ///
    /// Uses the canonical root established at construction when available;
    /// otherwise derives the project boundary from the file's parent directory.
    pub fn load(&self, path: &Path) -> Result<LoadedSource, ProjectLoadError> {
        let root = self
            .canonical_root
            .as_deref()
            .or_else(|| self.options.root())
            .unwrap_or_else(|| path.parent().unwrap_or_else(|| Path::new(".")));
        let boundary = self.boundary(root)?;
        match boundary.classify(path)? {
            PathClassification::Accepted(accepted) => {
                read_source(accepted.as_ref(), self.options.max_source_bytes())
            }
            PathClassification::Outside(path)
            | PathClassification::Excluded(path)
            | PathClassification::Unsupported(path) => {
                Err(ProjectLoadError::UnsupportedSource(path.into_path_buf()))
            }
        }
    }
}
