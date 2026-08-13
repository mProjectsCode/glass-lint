//! Phase 2–3: merged and compiled selection types.
//!
//! These types consume a typed predecessor; none use a mutable
//! `serde_json::Value` as its semantic model.

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use crate::tsconfig::{ParsedField, ParsedTsconfig};

// ---------------------------------------------------------------------------
// Phase 2 — Consuming inheritance / merged selection
// ---------------------------------------------------------------------------

/// Fully inherited (merged) selection data with plain string fields.
///
/// This is an intermediate type produced during config inheritance.
/// It exists only during construction and is consumed by
/// [`MergedSelection::compile`].
///
/// When a membership-controlling field (`files` or `include`) carried a
/// `WrongType` or `Null` value in the parsed config, the merge marks it as
/// invalid so that compilation can fail closed rather than broadening
/// membership by falling back to `**/*`.
pub struct MergedSelection {
    files: Option<Vec<String>>,
    include: Vec<String>,
    exclude: Vec<String>,
    invalid_controlling_field: bool,
    has_parent: bool,
}

impl MergedSelection {
    fn empty() -> Self {
        Self {
            files: None,
            include: Vec::new(),
            exclude: Vec::new(),
            invalid_controlling_field: false,
            has_parent: false,
        }
    }

    fn rebase_parent(&mut self, parent_directory: &Path, child_dir: &Path) {
        self.files = self
            .files
            .take()
            .map(|values| rebase_strings(values, parent_directory, child_dir));
        self.include = rebase_strings(
            std::mem::take(&mut self.include),
            parent_directory,
            child_dir,
        );
        self.exclude = rebase_strings(
            std::mem::take(&mut self.exclude),
            parent_directory,
            child_dir,
        );
        self.has_parent = true;
    }

    fn merge_child(mut self, child: ParsedTsconfig) -> Self {
        let ParsedTsconfig {
            extends: _,
            files,
            include,
            exclude,
            compiler_options_out_dir,
            compiler_options_declaration_dir,
            references: _,
            diagnostics: _,
        } = child;

        let files_invalid = !matches!(files, ParsedField::Present(_) | ParsedField::Absent);
        let include_invalid = !matches!(include, ParsedField::Present(_) | ParsedField::Absent);
        self.invalid_controlling_field |= files_invalid || include_invalid;

        self.files = if files_invalid {
            Some(Vec::new())
        } else {
            files.ok().or_else(|| self.files.take())
        };
        if self.files.is_some() && !files_invalid {
            self.include.clear();
            self.exclude.clear();
            return self;
        }

        self.include = if include_invalid {
            Vec::new()
        } else {
            match include {
                ParsedField::Present(values) => values,
                _ if self.has_parent => std::mem::take(&mut self.include),
                _ => vec!["**/*".to_string()],
            }
        };
        let child_exclude = match exclude {
            ParsedField::Present(values) => values,
            _ if self.has_parent => std::mem::take(&mut self.exclude),
            _ => Vec::new(),
        };
        self.exclude = Self::append_exclusions(
            child_exclude,
            compiler_options_out_dir,
            compiler_options_declaration_dir,
        );
        self
    }

    fn append_exclusions(
        mut exclude: Vec<String>,
        compiler_options_out_dir: ParsedField<String>,
        compiler_options_declaration_dir: ParsedField<String>,
    ) -> Vec<String> {
        for default in &["**/node_modules", "**/bower_components"] {
            if !exclude.iter().any(|entry| entry == default) {
                exclude.push(default.to_string());
            }
        }
        for output in [
            compiler_options_out_dir.ok(),
            compiler_options_declaration_dir.ok(),
        ]
        .into_iter()
        .flatten()
        {
            if !exclude.iter().any(|entry| entry == &output) {
                exclude.push(output);
            }
        }
        exclude
    }

    /// Consume this merged selection and compile it into a production
    /// selection. Raw include/exclude strings are consumed and discarded.
    pub(in crate::tsconfig) fn compile(self, config_path: PathBuf) -> CompiledTsconfigSelection {
        let Self {
            files,
            include,
            exclude,
            invalid_controlling_field,
            ..
        } = self;

        let pattern_set = TsconfigPatternSet::new(&include, &exclude, invalid_controlling_field);
        let pattern_diagnostics = pattern_set
            .invalid_patterns()
            .map(|pattern| format!("invalid glob pattern `{pattern}`"))
            .collect();

        CompiledTsconfigSelection {
            config_path,
            files,
            pattern_set,
            pattern_diagnostics,
        }
    }

    /// Explicit files list, or `None` when include/exclude select sources.
    #[cfg(test)]
    pub fn files(&self) -> Option<&[String]> {
        self.files.as_deref()
    }

    /// Include patterns relative to the config that declared them.
    #[cfg(test)]
    pub fn include(&self) -> &[String] {
        &self.include
    }

    /// Exclude patterns relative to the config that declared them.
    #[cfg(test)]
    pub fn exclude(&self) -> &[String] {
        &self.exclude
    }

    /// True when a membership-controlling field was invalid, so compilation
    /// must fail closed rather than broadening membership via `**/*`.
    #[cfg(test)]
    pub fn invalid_controlling_field(&self) -> bool {
        self.invalid_controlling_field
    }
}

/// A merged parent and the directory where its patterns were declared.
pub struct ParentSelection {
    merged: MergedSelection,
    parent_directory: PathBuf,
}

impl ParentSelection {
    pub fn new(merged: MergedSelection, parent_directory: PathBuf) -> Self {
        Self {
            merged,
            parent_directory,
        }
    }

    /// Consume this parent and merge the child config's selection fields into
    /// it, rebasing inherited paths to `child_dir`.
    pub fn merge(self, child: ParsedTsconfig, child_dir: &Path) -> MergedSelection {
        let Self {
            mut merged,
            parent_directory,
        } = self;
        merged.rebase_parent(&parent_directory, child_dir);
        merged.merge_child(child)
    }
}

/// Compute a relative path from `base` to `absolute`.
fn make_relative(absolute: &Path, base: &Path) -> PathBuf {
    let path_comps: Vec<_> = absolute.components().collect();
    let base_comps: Vec<_> = base.components().collect();
    let common_len = path_comps
        .iter()
        .zip(base_comps.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let mut result = PathBuf::new();
    for _ in common_len..base_comps.len() {
        result.push("..");
    }
    for comp in &path_comps[common_len..] {
        result.push(comp.as_os_str());
    }
    result
}

/// Remove `.` and `..` components from a path without consulting the
/// filesystem.  This is needed because glob-pattern paths (`**/*`) cannot
/// be canonicalized, yet `from_dir.join("../other/src/**/*")` leaves
/// unnormalized `parent/..` components that break `make_relative`.
fn normalize_safe(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut result = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                // Pop the last component only when there is a real
                // component to remove (don't pop past the root).
                let tail = result.components().next_back();
                if !matches!(tail, None | Some(Component::RootDir)) {
                    result.pop();
                }
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// Rebase a path/pattern from `from_dir` to be relative to `to_dir`.
fn rebase_path_pattern(pattern: &str, from_dir: &Path, to_dir: &Path) -> String {
    if Path::new(pattern).is_absolute() {
        return pattern.to_string();
    }
    let absolute = normalize_safe(&from_dir.join(pattern));
    let base = normalize_safe(to_dir);
    let relative = make_relative(&absolute, &base);
    relative.to_string_lossy().replace('\\', "/")
}

fn rebase_strings(items: Vec<String>, from_dir: &Path, to_dir: &Path) -> Vec<String> {
    items
        .into_iter()
        .map(|p| rebase_path_pattern(&p, from_dir, to_dir))
        .collect()
}

/// Merge a child [`ParsedTsconfig`] with an optional parent selection by
/// consuming both (moving owned fields).
/// No cloning of selection data occurs.
///
/// When a parent is provided, its bundled directory is the canonical
/// directory of the final parent config. Paths and patterns from the parent
/// are rebased before merging, so each path is interpreted relative to the
/// config file where it was declared.
pub fn merge_selection(
    child: ParsedTsconfig,
    parent: Option<ParentSelection>,
    child_dir: &Path,
) -> MergedSelection {
    match parent {
        Some(parent) => parent.merge(child, child_dir),
        None => MergedSelection::empty().merge_child(child),
    }
}

// ---------------------------------------------------------------------------
// Phase 3 — Compiled selection
// ---------------------------------------------------------------------------

/// An effective (fully inherited) tsconfig with normalized paths and compiled
/// patterns. This is the semantic model used for source selection.
///
/// Raw include/exclude strings are discarded after compilation; only the
/// compiled [`TsconfigPatternSet`] and the explicit `files` list are retained.
#[derive(Debug)]
pub struct CompiledTsconfigSelection {
    /// Canonical config path.
    config_path: PathBuf,
    /// Explicit files list (None = use include/exclude).
    files: Option<Vec<String>>,
    /// Compiled pattern set for include/exclude matching.
    pattern_set: TsconfigPatternSet,
    /// Invalid patterns that caused fail-closed source selection.
    pattern_diagnostics: Vec<String>,
}

impl CompiledTsconfigSelection {
    /// Borrow the canonical path of the config that produced this selection.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Borrow the normalized explicit file list, or `None` for pattern mode.
    pub fn explicit_files(&self) -> Option<&[String]> {
        self.files.as_deref()
    }

    /// Return whether a config-relative path is selected by include/exclude
    /// patterns. This operation owns slash normalization and is only valid
    /// for the pattern mode represented by `None` from
    /// [`Self::explicit_files`].
    pub fn includes(&self, relative: &Path) -> bool {
        let relative = relative.to_string_lossy();
        let normalized = if relative.contains('\\') {
            Cow::Owned(relative.replace('\\', "/"))
        } else {
            Cow::Borrowed(relative.as_ref())
        };
        self.pattern_set.is_included(&normalized)
    }

    /// Borrow diagnostics for invalid include or exclude patterns.
    pub fn pattern_diagnostics(&self) -> &[String] {
        &self.pattern_diagnostics
    }
}

// ---------------------------------------------------------------------------
// Compiled pattern set
// ---------------------------------------------------------------------------

/// Validated, normalized, and compiled include/exclude patterns. Provides
/// allocation-free borrowed matching against canonical project-relative paths.
///
/// When a membership-controlling field was invalid in the parsed config, the
/// entire pattern set is treated as empty so that no source is accepted until
/// the config is corrected.
#[derive(Clone, Debug)]
pub struct TsconfigPatternSet {
    includes: Vec<glob::Pattern>,
    excludes: Vec<glob::Pattern>,
    invalid: Vec<String>,
    fail_closed: bool,
}

fn matches_relative(pattern: &glob::Pattern, relative: &str) -> bool {
    pattern.matches(relative)
        || (!pattern.as_str().contains('/')
            && relative
                .split('/')
                .next_back()
                .is_some_and(|name| pattern.matches(name)))
}

impl TsconfigPatternSet {
    pub(in crate::tsconfig) fn new(
        includes: &[String],
        excludes: &[String],
        fail_closed: bool,
    ) -> Self {
        let normalize = |pattern: &str| -> String {
            let normalized = pattern.replace('\\', "/");
            if normalized.ends_with('/') {
                format!("{normalized}**/*")
            } else {
                normalized
            }
        };

        let compile = |patterns: &[String]| -> (Vec<glob::Pattern>, Vec<String>) {
            let mut compiled = Vec::new();
            let mut invalid = Vec::new();
            for pattern in patterns.iter().map(|p| normalize(p)) {
                match glob::Pattern::new(&pattern) {
                    Ok(pattern) => compiled.push(pattern),
                    Err(_) => invalid.push(pattern),
                }
            }
            (compiled, invalid)
        };

        let (includes, mut invalid) = compile(includes);
        let (excludes, exclude_invalid) = compile(excludes);
        invalid.extend(exclude_invalid);

        Self {
            includes,
            excludes,
            invalid,
            fail_closed,
        }
    }

    fn invalid_patterns(&self) -> impl Iterator<Item = &str> {
        self.invalid.iter().map(String::as_str)
    }

    /// Returns true when `relative` (a slash-normalized path relative to the
    /// config base) matches at least one include pattern and matches no exclude
    /// pattern. The path is borrowed; no allocation occurs.
    ///
    /// When any membership-controlling field was invalid the set returns false
    /// for every path, failing closed instead of broadening membership.
    pub fn is_included(&self, relative: &str) -> bool {
        if self.fail_closed || !self.invalid.is_empty() {
            return false;
        }
        let has_include_match = self
            .includes
            .iter()
            .any(|pattern| matches_relative(pattern, relative));
        if !has_include_match {
            return false;
        }
        !self
            .excludes
            .iter()
            .any(|pattern| matches_relative(pattern, relative))
    }
}
