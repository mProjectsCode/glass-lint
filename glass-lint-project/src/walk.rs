//! Shared bounded directory walking with filtering and budget enforcement.
//!
//! Both [`SourceCorpus`] and [`ProjectDiscovery`] independently configure
//! `WalkDir`, apply exclusions, count entries, and translate errors. This
//! module owns one authoritative walk-and-collect engine so that policy
//! (symlink handling, exclusion timing, visited/file budgets,
//! canonicalization, error conversion) has a single maintenance point.

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use walkdir::WalkDir;

use crate::{error::ProjectLoadError, options::ProjectLoadOptions};

/// Collect supported source files from a directory, bounded by the
/// configured visit and file limits.
///
/// When `deadline` is provided, the walk checks elapsed time between entry
/// iterations and returns [`ProjectLoadError::Timeout`] if the deadline
/// passes.
pub fn collect_files(
    options: &ProjectLoadOptions,
    root: &Path,
    deadline: Option<Instant>,
    include: &mut dyn FnMut(&Path) -> bool,
) -> Result<Vec<PathBuf>, ProjectLoadError> {
    let mut entries = Vec::new();
    let mut visited = 0usize;
    let walker = WalkDir::new(root)
        .follow_links(options.follow_symlinks)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            !entry.file_type().is_dir()
                || !options
                    .excluded_directories
                    .contains(&entry.file_name().to_string_lossy().to_string())
        });
    for entry in walker {
        if let Some(deadline) = deadline
            && Instant::now() >= deadline
        {
            return Err(ProjectLoadError::Timeout);
        }
        visited = visited.saturating_add(1);
        if visited > options.max_visited_entries {
            return Err(ProjectLoadError::TooManyEntries(
                options.max_visited_entries,
            ));
        }
        let entry = entry.map_err(|error| {
            let path = error.path().unwrap_or(root).to_path_buf();
            let message = error.to_string();
            let source = error
                .into_io_error()
                .unwrap_or_else(|| std::io::Error::other(message));
            ProjectLoadError::Io { path, source }
        })?;
        if entry.file_type().is_file() && options.supports(entry.path()) && include(entry.path()) {
            entries.push(entry.into_path());
            if entries.len() > options.max_files {
                return Err(ProjectLoadError::TooManyFiles(options.max_files));
            }
        }
    }
    Ok(entries)
}
