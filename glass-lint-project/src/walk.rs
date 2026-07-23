//! Shared bounded directory walking with filtering and budget enforcement.
//!
//! This module owns one authoritative walk-and-collect engine plus root
//! resolution so that filesystem policy (symlink handling on roots and
//! entries, exclusion timing, visited/file budgets, error conversion) has a
//! single maintenance point.

use std::{fs, path::Path, time::Instant};

use walkdir::WalkDir;

use crate::{
    admission::{AdmittedSourcePath, SourceAdmission},
    error::ProjectLoadError,
};

/// Resolve root metadata respecting the symlink-follow policy.
///
/// Returns `None` when the root is a symbolic link and
/// [`ProjectLoadOptions::follow_symlinks`] is `false`, signalling the caller
/// to skip this root.  Otherwise returns the metadata (possibly followed
/// through a symlink target) so the caller can distinguish a single file from
/// a directory before passing it to [`collect_files`].
pub fn resolve_root(
    options: &crate::options::ValidatedProjectLoadOptions,
    root: &Path,
) -> Result<Option<fs::Metadata>, ProjectLoadError> {
    let metadata = fs::symlink_metadata(root).map_err(|source| ProjectLoadError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() && !options.follow_symlinks() {
        return Ok(None);
    }
    let metadata = if metadata.file_type().is_symlink() {
        fs::metadata(root).map_err(|source| ProjectLoadError::Io {
            path: root.to_path_buf(),
            source,
        })?
    } else {
        metadata
    };
    Ok(Some(metadata))
}

/// Collect supported source files from a directory, bounded by the
/// configured visit and file limits.
///
/// When `deadline` is provided, the walk checks elapsed time between entry
/// iterations and returns [`ProjectLoadError::Timeout`] if the deadline
/// passes.
pub fn collect_files(
    admission: &SourceAdmission<'_>,
    root: &Path,
    deadline: Option<Instant>,
    include: &mut dyn FnMut(&Path) -> bool,
) -> Result<Vec<AdmittedSourcePath>, ProjectLoadError> {
    let mut entries = Vec::new();
    let mut visited = 0usize;
    let walker = WalkDir::new(root)
        .follow_links(admission.options().follow_symlinks())
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            !entry.file_type().is_dir()
                || !matches!(
                    admission.classify(entry.path()),
                    Ok(crate::admission::PathAdmission::Excluded(_))
                )
        });
    let mut budget = crate::admission::FileBudget::new(admission.options().max_files());
    for entry in walker {
        if let Some(deadline) = deadline
            && Instant::now() >= deadline
        {
            return Err(ProjectLoadError::Timeout);
        }
        visited = visited.saturating_add(1);
        if visited > admission.options().max_visited_entries() {
            return Err(ProjectLoadError::TooManyEntries(
                admission.options().max_visited_entries(),
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
        if entry.file_type().is_file()
            && include(entry.path())
            && let crate::admission::PathAdmission::Admitted(admitted) =
                admission.classify(entry.path())?
        {
            budget.try_admit()?;
            entries.push(admitted);
        }
    }
    Ok(entries)
}
