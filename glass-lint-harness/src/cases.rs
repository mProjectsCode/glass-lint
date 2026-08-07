//! Fixture discovery and case normalization.
//!
//! Case IDs and file order are normalized before execution so reports and
//! adapter requests remain stable across filesystem traversal implementations.

use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

use crate::types::Case;

mod project;
mod snippet;

#[cfg(test)]
mod tests;

#[cfg(test)]
use project::ProjectResolutionManifest;
use project::load_project_case;
use snippet::parse_case;

fn language_for_path(path: &Path) -> &'static str {
    match path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("ts" | "cts" | "mts") => "typescript",
        _ => "javascript",
    }
}

fn is_supported_fixture_filename(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some((_, extension)) = name.rsplit_once('.') else {
        return false;
    };
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "js" | "cjs" | "mjs" | "ts" | "cts" | "mts"
    ) && ![".d.ts", ".d.cts", ".d.mts"]
        .iter()
        .any(|suffix| name.to_ascii_lowercase().ends_with(suffix))
}

fn default_filename(path: &Path) -> String {
    path.file_name().map_or_else(
        || "main.js".into(),
        |name| name.to_string_lossy().into_owned(),
    )
}

pub fn load_cases(root: &Path) -> Result<Vec<Case>> {
    // Project manifests claim their whole directory; ordinary source files
    // beneath those directories must not be loaded as duplicate cases.
    let mut project_directories = BTreeSet::new();
    for entry in WalkDir::new(root) {
        let entry = entry?;
        if entry.file_type().is_file() && entry.file_name() == "case.toml" {
            project_directories.insert(entry.path().parent().unwrap_or(root).to_owned());
        }
    }
    let mut paths: Vec<_> = WalkDir::new(root)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| {
            entry.file_type().is_file()
                && is_supported_fixture_filename(entry.path())
                && !project_directories
                    .iter()
                    .any(|directory| entry.path().starts_with(directory))
        })
        .map(walkdir::DirEntry::into_path)
        .collect();
    paths.sort();

    let mut cases = paths
        .into_iter()
        .map(|path| {
            let source =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let case = parse_case(root, &path, source)
                .with_context(|| format!("parse {}", path.display()))?;
            let expected_language = language_for_path(&path);
            if case.language != expected_language {
                bail!(
                    "{}: language `{}` conflicts with its fixture extension (expected `{}`)",
                    path.display(),
                    case.language,
                    expected_language
                );
            }
            Ok(case)
        })
        .collect::<Result<Vec<_>>>()?;
    for directory in project_directories {
        cases.push(load_project_case(root, &directory)?);
    }
    cases.sort_by(|left, right| left.id.cmp(&right.id));
    let mut ids = BTreeSet::new();
    for case in &cases {
        if !ids.insert(case.id.clone()) {
            bail!("duplicate case id `{}`", case.id);
        }
    }
    Ok(cases)
}
