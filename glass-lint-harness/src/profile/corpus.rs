//! Corpus discovery, filtering, and deterministic sampling for profiling.

use std::{collections::BTreeSet, path::PathBuf};

use anyhow::{Context, Result};
use glass_lint_project::{SourceCorpus, ValidatedProjectLoadOptions};
use glob::{MatchOptions, Pattern};

/// Discover supported source files in deterministic path order.
pub fn discover_profile_files(
    roots: &[PathBuf],
    includes: &[String],
    excludes: &[String],
) -> Result<Vec<PathBuf>> {
    let includes = compile_globs(includes)?;
    let excludes = compile_globs(excludes)?;
    let corpus_options = ValidatedProjectLoadOptions::builder()
        .max_files(usize::MAX)
        .build()?;
    let corpus = SourceCorpus::from_validated(&corpus_options);
    let mut paths = BTreeSet::new();
    for root in roots {
        let found = corpus.discover_filtered(std::slice::from_ref(root), |path| {
            matches_filters(path, root, &includes, &excludes)
        })?;
        paths.extend(found);
    }
    Ok(paths.into_iter().collect())
}

/// Select a stable pseudo-random subset and restore path ordering afterwards.
pub fn sample_paths(paths: &mut Vec<PathBuf>, sample: usize, seed: u64) {
    if sample >= paths.len() {
        return;
    }
    let mut state = if seed == 0 {
        0x9e37_79b9_7f4a_7c15
    } else {
        seed
    };
    for index in (1..paths.len()).rev() {
        state ^= state << 7;
        state ^= state >> 9;
        state ^= state << 8;
        paths.swap(
            index,
            usize::try_from(state).unwrap_or(usize::MAX) % (index + 1),
        );
    }
    paths.truncate(sample);
    paths.sort();
}

fn matches_filters(
    path: &std::path::Path,
    root: &std::path::Path,
    includes: &[Pattern],
    excludes: &[Pattern],
) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative = relative.to_string_lossy().replace('\\', "/");
    let basename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let options = MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    };
    let matches = |patterns: &[Pattern]| {
        patterns.iter().any(|pattern| {
            pattern.matches_with(&relative, options)
                || (!pattern.as_str().contains('/') && pattern.matches_with(&basename, options))
        })
    };
    (includes.is_empty() || matches(includes)) && !matches(excludes)
}

fn compile_globs(patterns: &[String]) -> Result<Vec<Pattern>> {
    patterns
        .iter()
        .map(|pattern| {
            Pattern::new(pattern).with_context(|| format!("compile profiling glob {pattern}"))
        })
        .collect()
}
