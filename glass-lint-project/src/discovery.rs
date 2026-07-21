//! Filesystem membership and source loading.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use glass_lint_core::SourceFile;
use serde_json::Value;
use walkdir::{DirEntry, WalkDir};

use crate::{
    error::ProjectLoadError,
    options::{ProjectLoadOptions, ProjectSelection},
};

/// Discovers the bounded set of source files that belongs to a selection.
pub struct ProjectDiscovery<'a> {
    options: &'a ProjectLoadOptions,
    deadline: Option<Instant>,
}

impl<'a> ProjectDiscovery<'a> {
    /// Create a discovery view over validated loader options.
    pub fn with_deadline(options: &'a ProjectLoadOptions, deadline: Instant) -> Self {
        Self {
            options,
            deadline: Some(deadline),
        }
    }

    /// Borrow the discovery policy.
    pub fn options(&self) -> &ProjectLoadOptions {
        self.options
    }

    /// Resolve a selection into sorted, root-contained initial source paths.
    pub fn initial_paths(
        &self,
        selection: &ProjectSelection,
        selection_path: &Path,
        root: &Path,
    ) -> Result<Vec<PathBuf>, ProjectLoadError> {
        let mut paths = match selection {
            ProjectSelection::Entry(_) => self.entry_path(selection_path)?,
            ProjectSelection::Directory(_) => self.discover(selection_path)?,
            ProjectSelection::Tsconfig(config) => {
                if !selection_path.is_file() {
                    return Err(ProjectLoadError::SelectionNotFile(
                        selection_path.to_path_buf(),
                    ));
                }
                self.discover_tsconfig(config, selection_path.parent().unwrap_or(root))?
            }
        };

        self.validate_membership(&mut paths, selection_path, root)?;
        Ok(paths)
    }

    fn entry_path(&self, path: &Path) -> Result<Vec<PathBuf>, ProjectLoadError> {
        if !path.is_file() {
            return Err(ProjectLoadError::SelectionNotFile(path.to_path_buf()));
        }
        if !self.options.supports(path) {
            return Err(ProjectLoadError::UnsupportedSource(path.to_path_buf()));
        }
        Ok(vec![path.to_path_buf()])
    }

    fn validate_membership(
        &self,
        paths: &mut Vec<PathBuf>,
        selection: &Path,
        root: &Path,
    ) -> Result<(), ProjectLoadError> {
        let mut outside = false;
        paths.retain(|path| {
            if inside_root(root, path) {
                true
            } else {
                outside = true;
                false
            }
        });
        if outside {
            return Err(ProjectLoadError::SelectionOutsideRoot {
                selection: selection.to_path_buf(),
                root: root.to_path_buf(),
            });
        }
        paths.sort();
        paths.dedup();
        if paths.len() > self.options.max_files {
            return Err(ProjectLoadError::TooManyFiles(self.options.max_files));
        }
        Ok(())
    }

    fn discover(&self, directory: &Path) -> Result<Vec<PathBuf>, ProjectLoadError> {
        let mut entries = Vec::new();
        let mut visited = 0usize;
        let walker = WalkDir::new(directory)
            .follow_links(self.options.follow_symlinks)
            .sort_by_file_name();
        for entry in walker
            .into_iter()
            .filter_entry(|entry| self.accept_entry(entry))
        {
            self.check_timeout()?;
            visited = visited.saturating_add(1);
            if visited > self.options.max_visited_entries {
                return Err(ProjectLoadError::TooManyEntries(
                    self.options.max_visited_entries,
                ));
            }
            let entry = entry.map_err(|error| {
                let path = error.path().unwrap_or(directory).to_path_buf();
                let message = error.to_string();
                let source = error
                    .into_io_error()
                    .unwrap_or_else(|| std::io::Error::other(message));
                ProjectLoadError::Io { path, source }
            })?;
            if entry.file_type().is_file()
                && self.options.supports(entry.path())
            {
                entries.push(entry.into_path());
                if entries.len() > self.options.max_files {
                    return Err(ProjectLoadError::TooManyFiles(self.options.max_files));
                }
            }
        }
        Ok(entries)
    }

    fn discover_tsconfig(
        &self,
        config: &Path,
        directory: &Path,
    ) -> Result<Vec<PathBuf>, ProjectLoadError> {
        let mut visited = BTreeSet::new();
        let mut selected = BTreeSet::new();
        self.collect_tsconfig(&realpath(config)?, directory, &mut visited, &mut selected)?;
        Ok(selected.into_iter().collect())
    }

    fn collect_tsconfig(
        &self,
        config: &Path,
        fallback_directory: &Path,
        visited: &mut BTreeSet<PathBuf>,
        selected: &mut BTreeSet<PathBuf>,
    ) -> Result<(), ProjectLoadError> {
        let config = realpath(config)?;
        if !visited.insert(config.clone()) {
            return Ok(());
        }

        let parsed = read_tsconfig_path_extends(&config, fallback_directory, visited)?;
        let base = config.parent().unwrap_or(fallback_directory);
        let includes = patterns(&parsed, "include").unwrap_or_else(|| vec!["**/*"]);
        let mut excludes = patterns(&parsed, "exclude").unwrap_or_default();
        excludes.extend(["**/node_modules", "**/bower_components"]);
        Self::add_output_directories(&parsed, &mut excludes);

        if let Some(files) = parsed.get("files").and_then(Value::as_array) {
            self.add_explicit_files(base, files, selected)?;
        } else {
            self.add_matching_files(base, &includes, &excludes, selected)?;
        }
        self.collect_references(&parsed, base, visited, selected)
    }

    fn add_output_directories<'config>(config: &'config Value, excludes: &mut Vec<&'config str>) {
        if let Some(options) = config.get("compilerOptions") {
            for option in ["outDir", "declarationDir"] {
                if let Some(directory) = options.get(option).and_then(Value::as_str) {
                    excludes.push(directory);
                }
            }
        }
    }

    fn add_explicit_files(
        &self,
        base: &Path,
        files: &[Value],
        selected: &mut BTreeSet<PathBuf>,
    ) -> Result<(), ProjectLoadError> {
        for file in files.iter().filter_map(Value::as_str) {
            let path = base.join(file);
            if path.exists() && self.options.supports(&path) {
                selected.insert(realpath(&path)?);
            }
        }
        Ok(())
    }

    fn add_matching_files(
        &self,
        base: &Path,
        includes: &[&str],
        excludes: &[&str],
        selected: &mut BTreeSet<PathBuf>,
    ) -> Result<(), ProjectLoadError> {
        for path in self.discover(base)? {
            let relative = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if includes
                .iter()
                .any(|pattern| tsconfig_pattern_matches(pattern, &relative))
                && !excludes
                    .iter()
                    .any(|pattern| tsconfig_pattern_matches(pattern, &relative))
            {
                selected.insert(realpath(&path)?);
            }
        }
        Ok(())
    }

    fn collect_references(
        &self,
        config: &Value,
        base: &Path,
        visited: &mut BTreeSet<PathBuf>,
        selected: &mut BTreeSet<PathBuf>,
    ) -> Result<(), ProjectLoadError> {
        let Some(references) = config.get("references").and_then(Value::as_array) else {
            return Ok(());
        };
        for reference in references {
            let Some(path) = reference.get("path").and_then(Value::as_str) else {
                continue;
            };
            let mut target = base.join(path);
            if target.is_dir() {
                target = target.join("tsconfig.json");
            }
            if target.exists() {
                self.collect_tsconfig(&target, base, visited, selected)?;
            }
        }
        Ok(())
    }

    fn accept_entry(&self, entry: &DirEntry) -> bool {
        !entry.file_type().is_dir()
            || entry
                .file_name()
                .to_str()
                .is_none_or(|name| !self.options.excluded_directories.contains(name))
    }

    pub fn read_source(&self, root: &Path, path: &Path) -> Result<SourceFile, ProjectLoadError> {
        let canonical_root = realpath(root)?;
        let canonical_path = realpath(path)?;
        if !inside_root(&canonical_root, &canonical_path) {
            return Err(ProjectLoadError::SelectionOutsideRoot {
                selection: path.to_path_buf(),
                root: canonical_root,
            });
        }
        crate::corpus::SourceCorpus::new_unchecked(self.options)
            .load_source_file_at(&canonical_root, &canonical_path)
    }

    fn check_timeout(&self) -> Result<(), ProjectLoadError> {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() > deadline)
        {
            return Err(ProjectLoadError::Timeout);
        }
        Ok(())
    }
}

fn patterns<'a>(config: &'a Value, key: &str) -> Option<Vec<&'a str>> {
    config
        .get(key)
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>())
}

/// Materialize inherited options before selecting runtime sources.
fn read_tsconfig_path_extends(
    config: &Path,
    fallback_directory: &Path,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<Value, ProjectLoadError> {
    let mut text = fs::read_to_string(config).map_err(|source| ProjectLoadError::Io {
        path: config.to_path_buf(),
        source,
    })?;
    json_strip_comments::strip(&mut text).map_err(|error| parse_error(config, error))?;
    let parsed: Value = serde_json::from_str(&text).map_err(|error| parse_error(config, error))?;
    let mut effective = Value::Object(serde_json::Map::new());
    if let Some(extends) = parsed.get("extends").and_then(Value::as_str)
        && let Some(parent) = resolve_tsconfig_extends(config, extends, fallback_directory)
        && parent.exists()
    {
        let parent = realpath(&parent)?;
        if visited.insert(parent.clone()) {
            effective = read_tsconfig_path_extends(
                &parent,
                parent.parent().unwrap_or(fallback_directory),
                visited,
            )?;
        }
    }
    merge_tsconfig_inheritance(&mut effective, parsed);
    Ok(effective)
}

/// Make a selection path absolute without requiring it to exist yet.
pub fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}

/// Canonicalize a path and preserve loader-specific I/O context on failure.
pub fn realpath(path: &Path) -> Result<PathBuf, ProjectLoadError> {
    fs::canonicalize(path).map_err(|source| ProjectLoadError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Test lexical containment in the canonical project-root namespace.
pub fn inside_root(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok()
}

fn parse_error(config: &Path, error: impl std::fmt::Display) -> ProjectLoadError {
    ProjectLoadError::InvalidOptions(crate::ProjectOptionError::Message(format!(
        "parse {}: {error}",
        config.display()
    )))
}

fn tsconfig_pattern_matches(pattern: &str, relative: &str) -> bool {
    let pattern = pattern.replace('\\', "/");
    let pattern = if pattern.ends_with('/') {
        format!("{pattern}**/*")
    } else {
        pattern
    };
    glob::Pattern::new(&pattern).is_ok_and(|pattern| {
        pattern.matches(relative)
            || (!pattern.as_str().contains('/')
                && relative
                    .split('/')
                    .next_back()
                    .is_some_and(|name| pattern.matches(name)))
    })
}

fn resolve_tsconfig_extends(
    config: &Path,
    extends: &str,
    fallback_directory: &Path,
) -> Option<PathBuf> {
    // Package-based `extends` is resolver policy rather than source
    // membership. Relative and absolute configs avoid admitting dependency
    // sources by accident.
    if !extends.starts_with('.') && !Path::new(extends).is_absolute() {
        return None;
    }
    let base = config.parent().unwrap_or(fallback_directory);
    let mut path = if Path::new(extends).is_absolute() {
        PathBuf::from(extends)
    } else {
        base.join(extends)
    };
    if path.extension().is_none() {
        path.set_extension("json");
    }
    Some(path)
}

fn merge_tsconfig_inheritance(base: &mut Value, child: Value) {
    match (base, child) {
        (Value::Object(base), Value::Object(child)) => {
            for (key, value) in child {
                if let Some(existing) = base.get_mut(&key) {
                    if key == "compilerOptions" {
                        merge_tsconfig_inheritance(existing, value);
                    } else {
                        *existing = value;
                    }
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, child) => *base = child,
    }
}
