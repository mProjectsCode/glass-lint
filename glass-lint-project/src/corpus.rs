//! Bounded source corpus discovery and loading without project linking.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use glass_lint_core::{SourceFile, SourceLanguage};
use walkdir::WalkDir;

use crate::{error::ProjectLoadError, options::ProjectLoadOptions};

#[derive(Clone, Debug)]
pub struct CorpusFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub source: String,
}

pub struct SourceCorpus<'a> {
    options: &'a ProjectLoadOptions,
}

impl<'a> SourceCorpus<'a> {
    pub fn new(options: &'a ProjectLoadOptions) -> Result<Self, ProjectLoadError> {
        options.validate()?;
        Ok(Self { options })
    }

    pub fn discover(&self, roots: &[PathBuf]) -> Result<Vec<PathBuf>, ProjectLoadError> {
        self.discover_filtered(roots, |_| true)
    }

    pub fn discover_filtered(
        &self,
        roots: &[PathBuf],
        mut include: impl FnMut(&Path) -> bool,
    ) -> Result<Vec<PathBuf>, ProjectLoadError> {
        let mut paths = BTreeSet::new();
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
                if supported_path(root, &self.options.extensions) && include(root) {
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
                let entry = entry.map_err(|error| ProjectLoadError::Io {
                    path: error.path().unwrap_or(root).to_path_buf(),
                    source: error
                        .into_io_error()
                        .unwrap_or_else(|| std::io::Error::other("directory traversal failed")),
                })?;
                if entry.file_type().is_file()
                    && supported_path(entry.path(), &self.options.extensions)
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

    pub fn load(&self, path: &Path) -> Result<CorpusFile, ProjectLoadError> {
        if !supported_path(path, &self.options.extensions) {
            return Err(ProjectLoadError::UnsupportedSource(path.to_path_buf()));
        }
        let metadata = fs::metadata(path).map_err(|source| ProjectLoadError::Io {
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
        let source = fs::read_to_string(path).map_err(|source| ProjectLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(CorpusFile {
            path: path.to_path_buf(),
            bytes: metadata.len(),
            source,
        })
    }

    pub fn load_source_file(
        &self,
        root: &Path,
        path: &Path,
    ) -> Result<SourceFile, ProjectLoadError> {
        let file = self.load(path)?;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        Ok(SourceFile {
            language: SourceLanguage::from_filename(&relative),
            path: relative,
            source: file.source,
        })
    }
}

pub fn supported_path(path: &Path, extensions: &[String]) -> bool {
    let name = path.to_string_lossy().to_ascii_lowercase();
    extensions
        .iter()
        .any(|extension| name.ends_with(&extension.to_ascii_lowercase()))
        && ![".d.ts", ".d.cts", ".d.mts"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
}
