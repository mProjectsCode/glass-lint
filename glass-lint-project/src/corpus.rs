//! Bounded source corpus discovery and loading without project linking.

use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use crate::{
    admission::SourceAdmission,
    error::ProjectLoadError,
    options::{ProjectLoadOptions, ValidatedProjectLoadOptions},
    walk,
};

#[derive(Clone, Debug)]
pub struct CorpusFile {
    /// Filesystem path retained for diagnostics and profiling.
    pub path: PathBuf,
    /// Byte length measured before decoding.
    pub bytes: u64,
    /// UTF-8 source text loaded under the configured byte limit.
    pub source: String,
}

/// Read raw source bytes from a trusted path with a byte budget.
///
/// This is the lowest-level read operation; callers must validate
/// extension support and containment before calling this function.
pub fn read_source_bytes(
    path: &Path,
    max_source_bytes: u64,
) -> Result<CorpusFile, ProjectLoadError> {
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
    Ok(CorpusFile {
        path: path.to_path_buf(),
        bytes: source.len() as u64,
        source,
    })
}

pub struct SourceCorpus {
    options: ValidatedProjectLoadOptions,
}

impl SourceCorpus {
    /// Validate options and create a corpus view without performing I/O.
    pub fn new(options: &ProjectLoadOptions) -> Result<Self, ProjectLoadError> {
        Ok(Self {
            options: options.clone().validated()?,
        })
    }

    /// Create a corpus from a policy already checked at the loader boundary.
    pub fn from_validated(options: &ValidatedProjectLoadOptions) -> Self {
        Self {
            options: options.clone(),
        }
    }

    /// Create a corpus view without re-validating options. Only use when
    /// options are already known to be valid (e.g., after
    /// `ValidatedProjectLoadOptions`).
    pub fn new_unchecked(options: &ProjectLoadOptions) -> Self {
        Self {
            options: options
                .clone()
                .validated()
                .expect("validated corpus options"),
        }
    }

    /// Discover supported files in deterministic path order.
    pub fn discover(&self, roots: &[PathBuf]) -> Result<Vec<PathBuf>, ProjectLoadError> {
        self.discover_filtered(roots, |_| true)
    }

    /// Discover files while applying a caller-owned membership predicate.
    pub fn discover_filtered(
        &self,
        roots: &[PathBuf],
        mut include: impl FnMut(&Path) -> bool,
    ) -> Result<Vec<PathBuf>, ProjectLoadError> {
        let mut paths = BTreeSet::new();
        for root in roots {
            let Some(metadata) = walk::resolve_root(&self.options, root)? else {
                continue;
            };
            let admission = SourceAdmission::new(root, &self.options)?;
            if metadata.is_file() {
                if admission.admitted_path(root)?.is_some() && include(root) {
                    paths.insert(admission.canonicalize(root)?.into_path_buf());
                }
                continue;
            }
            if !metadata.is_dir() {
                return Err(ProjectLoadError::InvalidOptions(
                    crate::ProjectOptionError::Message(format!(
                        "corpus root is not a file or directory: {}",
                        root.display()
                    )),
                ));
            }
            let found = walk::collect_files(&admission, root, None, &mut include)?;
            for path in found {
                paths.insert(path);
                if paths.len() > self.options.max_files() {
                    return Err(ProjectLoadError::TooManyFiles(self.options.max_files()));
                }
            }
        }
        debug_assert!(paths.len() <= self.options.max_files());
        Ok(paths.into_iter().collect())
    }

    /// Read one supported source file after enforcing the byte budget.
    pub fn load(&self, path: &Path) -> Result<CorpusFile, ProjectLoadError> {
        let root = path.parent().unwrap_or_else(|| Path::new("."));
        let admission = SourceAdmission::new(root, &self.options)?;
        let Some(path) = admission.admitted_path(path)? else {
            return Err(ProjectLoadError::UnsupportedSource(path.to_path_buf()));
        };
        read_source_bytes(path.as_ref(), self.options.max_source_bytes())
    }
}
