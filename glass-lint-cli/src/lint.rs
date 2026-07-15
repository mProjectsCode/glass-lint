//! Filesystem discovery and per-file lint execution.

use crate::{
    args::Command,
    config::{self, Config},
    output::{FileOutput, Summary},
};
use anyhow::{Context, Result, bail};
use glass_lint_core::{Linter, SourceLanguage};
use std::{fs, path::PathBuf};
use walkdir::WalkDir;

pub fn run(config: &Config, command: Command) -> Result<bool> {
    let linter = config::selected_linter(config)?;
    let (path, require_file) = match command {
        Command::Check { path } => (path, false),
        Command::Snippet { path } => (path, true),
        Command::Rules => unreachable!("rules are handled before lint execution"),
    };

    if require_file && !path.is_file() {
        bail!("snippet path is not a file: {}", path.display())
    }

    let paths = discover_paths(&path)?;
    if paths.is_empty() {
        bail!(
            "no JavaScript or TypeScript files found at {}",
            path.display()
        )
    }

    lint_files(config, &linter, paths)
}

fn discover_paths(path: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut paths = if path.is_dir() {
        tracing::debug!(target: "glass_lint::cli", path = %path.display(), "discovery started");
        WalkDir::new(path)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|entry| {
                entry.file_type().is_file()
                    && SourceLanguage::is_supported_filename(&entry.path().to_string_lossy())
            })
            .map(walkdir::DirEntry::into_path)
            .collect()
    } else {
        vec![path.clone()]
    };
    paths.sort();
    tracing::debug!(target: "glass_lint::cli", files = paths.len(), "discovery completed");
    Ok(paths)
}

fn lint_files(config: &Config, linter: &Linter, paths: Vec<PathBuf>) -> Result<bool> {
    let mut files = Vec::with_capacity(paths.len());
    let mut failed = false;

    for path in paths {
        let metadata =
            fs::metadata(&path).with_context(|| format!("inspect {}", path.display()))?;
        if metadata.len() > config.cli.max_bytes {
            bail!(
                "{} exceeds max_bytes {}",
                path.display(),
                config.cli.max_bytes
            )
        }

        let source =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let name = path.to_string_lossy().into_owned();
        tracing::debug!(
            target: "glass_lint::cli",
            path = %name,
            bytes = source.len(),
            "file inspected"
        );

        let report = linter.lint(&source, &name);
        failed |= !report.parse_diagnostics.is_empty()
            || report
                .findings
                .iter()
                .any(|finding| config.cli.fail_on.fails(finding.severity));
        files.push(FileOutput {
            path: name,
            report,
            source,
        });
    }

    let summary = Summary {
        files: files.len(),
        findings: files.iter().map(|file| file.report.findings.len()).sum(),
        parse_diagnostics: files
            .iter()
            .map(|file| file.report.parse_diagnostics.len())
            .sum(),
    };
    crate::output::write_report(config, &files, summary)?;
    tracing::info!(target: "glass_lint::cli", files = files.len(), "command completed");
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::discover_paths;
    use std::fs;

    #[test]
    fn discovers_sorted_runtime_javascript_and_typescript_files() {
        let root =
            std::env::temp_dir().join(format!("glass-lint-cli-discovery-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        for filename in ["z.ts", "a.mjs", "c.d.ts", "b.cts", "ignored.txt"] {
            fs::write(root.join(filename), "").unwrap();
        }

        let paths = discover_paths(&root).unwrap();
        let names: Vec<_> = paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.mjs", "b.cts", "z.ts"]);

        fs::remove_dir_all(root).unwrap();
    }
}
