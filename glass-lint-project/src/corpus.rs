//! Bounded source corpus discovery and loading without project linking.

use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use glass_lint_core::SourceFile;
use walkdir::WalkDir;

use crate::{error::ProjectLoadError, options::ProjectLoadOptions};

#[derive(Clone, Debug)]
pub struct CorpusFile {
    /// Filesystem path retained for diagnostics and profiling.
    pub path: PathBuf,
    /// Byte length measured before decoding.
    pub bytes: u64,
    /// UTF-8 source text loaded under the configured byte limit.
    pub source: String,
}

pub struct SourceCorpus<'a> {
    options: &'a ProjectLoadOptions,
}

impl<'a> SourceCorpus<'a> {
    /// Validate options and create a corpus view without performing I/O.
    pub fn new(options: &'a ProjectLoadOptions) -> Result<Self, ProjectLoadError> {
        options.validate()?;
        Ok(Self { options })
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
        let mut visited = 0usize;
        for root in roots {
            let metadata = fs::symlink_metadata(root).map_err(|source| ProjectLoadError::Io {
                path: root.clone(),
                source,
            })?;
            if metadata.file_type().is_symlink() && !self.options.follow_symlinks {
                continue;
            }
            let metadata = if metadata.file_type().is_symlink() {
                fs::metadata(root).map_err(|source| ProjectLoadError::Io {
                    path: root.clone(),
                    source,
                })?
            } else {
                metadata
            };
            if metadata.is_file() {
                if is_supported_runtime_source(root, &self.options.extensions) && include(root) {
                    paths.insert(root.clone());
                }
                continue;
            }
            if !metadata.is_dir() {
                return Err(ProjectLoadError::InvalidOptions(format!(
                    "corpus root is not a file or directory: {}",
                    root.display()
                )));
            }
            let walker = WalkDir::new(root)
                .follow_links(self.options.follow_symlinks)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|entry| {
                    !entry.file_type().is_dir()
                        || !self
                            .options
                            .excluded_directories
                            .contains(&entry.file_name().to_string_lossy().to_string())
                });
            for entry in walker {
                visited = visited.saturating_add(1);
                if visited > self.options.max_visited_entries {
                    return Err(ProjectLoadError::TooManyEntries(
                        self.options.max_visited_entries,
                    ));
                }
                let entry = entry.map_err(|error| ProjectLoadError::Io {
                    path: error.path().unwrap_or(root).to_path_buf(),
                    source: error
                        .into_io_error()
                        .unwrap_or_else(|| std::io::Error::other("directory traversal failed")),
                })?;
                if entry.file_type().is_file()
                    && is_supported_runtime_source(entry.path(), &self.options.extensions)
                    && include(entry.path())
                {
                    paths.insert(entry.into_path());
                    if paths.len() > self.options.max_files {
                        return Err(ProjectLoadError::TooManyFiles(self.options.max_files));
                    }
                }
            }
        }
        if paths.len() > self.options.max_files {
            return Err(ProjectLoadError::TooManyFiles(self.options.max_files));
        }
        Ok(paths.into_iter().collect())
    }

    /// Read one supported source file after enforcing the byte budget.
    pub fn load(&self, path: &Path) -> Result<CorpusFile, ProjectLoadError> {
        if !is_supported_runtime_source(path, &self.options.extensions) {
            return Err(ProjectLoadError::UnsupportedSource(path.to_path_buf()));
        }
        let file = fs::File::open(path).map_err(|source| ProjectLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let metadata = file.metadata().map_err(|source| ProjectLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if metadata.len() > self.options.max_source_bytes {
            return Err(ProjectLoadError::SourceTooLarge {
                path: path.to_path_buf(),
                bytes: metadata.len(),
                limit: self.options.max_source_bytes,
            });
        }
        let mut bytes = Vec::new();
        file.take(self.options.max_source_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|source| ProjectLoadError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if bytes.len() as u64 > self.options.max_source_bytes {
            return Err(ProjectLoadError::SourceTooLarge {
                path: path.to_path_buf(),
                bytes: bytes.len() as u64,
                limit: self.options.max_source_bytes,
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

    /// Convert a loaded filesystem path into a normalized core source record.
    pub fn load_source_file(
        &self,
        root: &Path,
        path: &Path,
    ) -> Result<SourceFile, ProjectLoadError> {
        let canonical_root = fs::canonicalize(root).map_err(|source| ProjectLoadError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let canonical_path = fs::canonicalize(path).map_err(|source| ProjectLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if canonical_path.strip_prefix(&canonical_root).is_err() {
            return Err(ProjectLoadError::SelectionOutsideRoot {
                selection: path.to_path_buf(),
                root: canonical_root,
            });
        }
        let file = self.load(&canonical_path)?;
        let relative = canonical_path
            .strip_prefix(&canonical_root)
            .unwrap_or(&canonical_path)
            .to_string_lossy()
            .replace('\\', "/");
        Ok(SourceFile::new(relative, file.source)?)
    }
}

pub fn is_supported_runtime_source(path: &Path, extensions: &[String]) -> bool {
    let name = path.to_string_lossy().to_ascii_lowercase();
    extensions
        .iter()
        .any(|extension| name.ends_with(&extension.to_ascii_lowercase()))
        && ![".d.ts", ".d.cts", ".d.mts"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
}
