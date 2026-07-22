//! Canonical project-root and filesystem admission boundary.
//!
//! Every accepted path must pass through one [`SourceAdmission`]; containment,
//! exclusion, extension-support, and canonicalization all have one
//! authoritative implementation here.

use std::{
    fs,
    path::{Path, PathBuf},
};

use glass_lint_core::SourceFile;

use crate::{
    corpus::read_source_bytes,
    error::ProjectLoadError,
    options::ProjectLoadOptions,
};

/// Owns the canonical project root and source-file admission policy.
///
/// Construct one [`SourceAdmission`] per project; its canonical root is
/// resolved once and shared by discovery, resolution, and loading.
pub struct SourceAdmission<'a> {
    canonical_root: PathBuf,
    options: &'a ProjectLoadOptions,
}

impl<'a> SourceAdmission<'a> {
    /// Establish one canonical root before any file I/O.
    pub fn new(root: &Path, options: &'a ProjectLoadOptions) -> Result<Self, ProjectLoadError> {
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
    pub fn options(&self) -> &ProjectLoadOptions {
        self.options
    }

    /// Resolve a path to its canonical form through the filesystem.
    pub fn canonicalize(&self, path: &Path) -> Result<PathBuf, ProjectLoadError> {
        realpath(path)
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

    /// Test whether a file extension is supported by the loader policy.
    pub fn supports(&self, path: &Path) -> bool {
        self.options.supports(path)
    }

    /// Test whether a path under the root has an excluded directory ancestor.
    pub fn is_excluded(&self, path: &Path) -> bool {
        self.options.excludes_path(&self.canonical_root, path)
    }

    /// Compute the project-relative, slash-normalized display path.
    pub fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.canonical_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Canonicalize, check containment and support, read, and produce a
    /// normalized [`SourceFile`] in one pass.
    ///
    /// This is the single entry-point for loading an admitted source file.
    pub fn load_source_file(&self, path: &Path) -> Result<SourceFile, ProjectLoadError> {
        let canonical_path = self.canonicalize(path)?;
        self.check_inside_root(&canonical_path)?;
        if !self.supports(&canonical_path) {
            return Err(ProjectLoadError::UnsupportedSource(canonical_path));
        }
        let corpus_file = read_source_bytes(&canonical_path, self.options.max_source_bytes)?;
        let relative = self.relative_path(&canonical_path);
        SourceFile::new(relative, corpus_file.source).map_err(Into::into)
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
pub fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}
