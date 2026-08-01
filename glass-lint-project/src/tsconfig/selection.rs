//! Phase 2–3: merged and compiled selection types.
//!
//! These types consume a typed predecessor; none use a mutable
//! `serde_json::Value` as its semantic model.

use std::path::{Path, PathBuf};

use crate::tsconfig::{ParsedField, ParsedTsconfig};

// ---------------------------------------------------------------------------
// Phase 2 — Consuming inheritance / merged selection
// ---------------------------------------------------------------------------

/// Fully inherited (merged) selection data with plain string fields.
///
/// This is an intermediate type produced during config inheritance.
/// It exists only during construction and is consumed by
/// [`CompiledTsconfigSelection::compile`].
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
}

/// A merged parent and the directory where its patterns were declared.
pub struct ParentSelection {
    selection: MergedSelection,
    directory: PathBuf,
}

impl ParentSelection {
    pub fn new(selection: MergedSelection, directory: PathBuf) -> Self {
        Self {
            selection,
            directory,
        }
    }

    pub fn into_parts(self) -> (MergedSelection, PathBuf) {
        (self.selection, self.directory)
    }
}

impl MergedSelection {
    pub fn into_parts(self) -> (Option<Vec<String>>, Vec<String>, Vec<String>, bool) {
        (
            self.files,
            self.include,
            self.exclude,
            self.invalid_controlling_field,
        )
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
    // Destructure-and-merge in one pass: the destructuring binding of
    // ParsedTsconfig paired with the field-by-field inheritance rule
    // (child wins, then parent, then default) is clearest when every
    // field is visible in the same scope.
    let ParsedTsconfig {
        extends: _,
        files: child_files,
        include: child_include,
        exclude: child_exclude,
        compiler_options_out_dir,
        compiler_options_declaration_dir,
        references: _,
        diagnostics: _,
    } = child;

    let has_parent = parent.is_some();
    // Rebase parent paths from the parent directory to child_dir so every
    // path is interpreted relative to the config that declared it.
    let (parent_files, parent_include, parent_exclude, parent_invalid) = parent.map_or_else(
        || (None, Vec::new(), Vec::new(), false),
        |m| {
            let (selection, pdir) = m.into_parts();
            (
                selection.files.map(|v| rebase_strings(v, &pdir, child_dir)),
                rebase_strings(selection.include, &pdir, child_dir),
                rebase_strings(selection.exclude, &pdir, child_dir),
                selection.invalid_controlling_field,
            )
        },
    );
    // Track whether a controlling field was invalid so that compilation can
    // fail closed instead of broadening membership via `**/*` fallback.
    let files_invalid = !matches!(child_files, ParsedField::Present(_) | ParsedField::Absent);
    let include_invalid = !matches!(child_include, ParsedField::Present(_) | ParsedField::Absent);
    let invalid_controlling_field = parent_invalid || files_invalid || include_invalid;

    let files = if files_invalid {
        Some(Vec::new())
    } else {
        child_files.ok().or(parent_files)
    };

    let (include, exclude) = if files.is_some() && !files_invalid {
        (Vec::new(), Vec::new())
    } else {
        let include = if include_invalid {
            Vec::new()
        } else {
            match child_include {
                ParsedField::Present(v) => v,
                _ if has_parent => parent_include,
                _ => vec!["**/*".to_string()],
            }
        };

        let mut exclude = match child_exclude {
            ParsedField::Present(v) => v,
            _ if has_parent => parent_exclude,
            _ => Vec::new(),
        };
        // Always add default runtime exclusions
        for default in &["**/node_modules", "**/bower_components"] {
            if !exclude.iter().any(|e| e == default) {
                exclude.push(default.to_string());
            }
        }
        // Add output directories from this config's compilerOptions
        if let Some(out_dir) = compiler_options_out_dir.ok()
            && !exclude.iter().any(|e| e == &out_dir)
        {
            exclude.push(out_dir);
        }
        if let Some(decl_dir) = compiler_options_declaration_dir.ok()
            && !exclude.iter().any(|e| e == &decl_dir)
        {
            exclude.push(decl_dir);
        }

        (include, exclude)
    };

    MergedSelection {
        files,
        include,
        exclude,
        invalid_controlling_field,
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
    pub config_path: PathBuf,
    /// Explicit files list (None = use include/exclude).
    pub files: Option<Vec<String>>,
    /// Compiled pattern set for include/exclude matching.
    pub pattern_set: TsconfigPatternSet,
    /// Invalid patterns that caused fail-closed source selection.
    pub pattern_diagnostics: Vec<String>,
}

impl CompiledTsconfigSelection {
    /// Compile a merged selection into a production selection.
    /// Raw include/exclude strings are consumed and discarded.
    pub(in crate::tsconfig) fn compile(config_path: PathBuf, merged: MergedSelection) -> Self {
        let (files, include, exclude, invalid_controlling_field) = merged.into_parts();

        let pattern_set = TsconfigPatternSet::new(&include, &exclude, invalid_controlling_field);
        let pattern_diagnostics = pattern_set
            .invalid_patterns()
            .map(|pattern| format!("invalid glob pattern `{pattern}`"))
            .collect();

        Self {
            config_path,
            files,
            pattern_set,
            pattern_diagnostics,
        }
    }
}

// ---------------------------------------------------------------------------
// Compiled pattern set
// ---------------------------------------------------------------------------

/// Validated, normalized, and compiled include/exclude patterns. Provides
/// allocation-free borrowed matching against canonical project-relative paths.
///
/// When a membership-controlling field was invalid in the parsed config, the
/// entire pattern set is treated as empty so that no source is admitted until
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
