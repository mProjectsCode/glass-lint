//! Filesystem discovery and per-file lint execution.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use glass_lint_core::Linter;
use glass_lint_project::{ProjectLoadOptions, ProjectLoader, ProjectSelection, SourceCorpus};

use crate::{
    args::Command,
    config::{self, Config},
    output::{FileOutput, Summary},
};

/// Execute a linting command and return whether its findings should fail CI.
///
/// Operational failures are returned as `Err`; a successful lint with findings
/// is represented by `Ok(true)` so the binary can distinguish exit status 1
/// from an invocation or I/O error.
pub fn run(config: &Config, command: Command) -> Result<bool> {
    // Project checks use the resolver-aware path; snippets intentionally lint
    // discovered files independently and therefore do not link modules.
    let linter = config::selected_linter(config)?;
    match command {
        Command::Check { path } => lint_project(config, &linter, &path),
        Command::Snippet { path } => {
            if !path.is_file() {
                bail!("snippet path is not a file: {}", path.display())
            }
            let options = ProjectLoadOptions {
                max_source_bytes: config.cli.max_bytes,
                ..ProjectLoadOptions::default()
            };
            let corpus = SourceCorpus::new(&options).map_err(|error| anyhow::anyhow!(error))?;
            let paths = corpus
                .discover(std::slice::from_ref(&path))
                .map_err(|error| anyhow::anyhow!(error))?;
            if paths.is_empty() {
                bail!(
                    "no JavaScript or TypeScript files found at {}",
                    path.display()
                )
            }
            lint_files(config, &linter, paths)
        }
        Command::Rules => unreachable!("rules are handled before lint execution"),
    }
}

fn lint_project(config: &Config, linter: &Linter, path: &std::path::Path) -> Result<bool> {
    let selection = if path.is_dir() {
        ProjectSelection::directory(path.to_path_buf())
    } else if path.file_name().is_some_and(|name| name == "tsconfig.json") {
        ProjectSelection::tsconfig(path.to_path_buf())
    } else {
        ProjectSelection::entry(path.to_path_buf())
    };
    let options = ProjectLoadOptions {
        max_source_bytes: config.cli.max_bytes,
        ..ProjectLoadOptions::default()
    };
    let loader = ProjectLoader::new(options).map_err(|error| anyhow::anyhow!(error))?;
    let report = loader
        .load_and_lint(linter, &selection)
        .with_context(|| format!("analyze project at {}", path.display()))?;
    let failed = !report.diagnostics.is_empty()
        || report.files.iter().any(|file| {
            !file.parse_diagnostics.is_empty()
                || file
                    .findings
                    .iter()
                    .any(|finding| config.cli.fail_on.fails(finding.severity))
        });
    crate::output::write_project_report(config, &report)?;
    tracing::info!(target: "glass_lint::cli", files = report.files.len(), "project command completed");
    Ok(failed)
}

fn lint_files(config: &Config, linter: &Linter, paths: Vec<PathBuf>) -> Result<bool> {
    let options = ProjectLoadOptions {
        max_source_bytes: config.cli.max_bytes,
        ..ProjectLoadOptions::default()
    };
    let corpus = SourceCorpus::new(&options).map_err(|error| anyhow::anyhow!(error))?;
    let mut files = Vec::with_capacity(paths.len());
    let mut failed = false;

    for path in paths {
        let source = corpus
            .load(&path)
            .map_err(|error| anyhow::anyhow!(error))?
            .source;
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
    use std::fs;

    use glass_lint_project::{ProjectLoadOptions, SourceCorpus};

    #[test]
    fn discovers_sorted_runtime_javascript_and_typescript_files() {
        let root =
            std::env::temp_dir().join(format!("glass-lint-cli-discovery-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        for filename in ["z.ts", "a.mjs", "c.d.ts", "b.cts", "ignored.txt"] {
            fs::write(root.join(filename), "").unwrap();
        }

        let paths = SourceCorpus::new(&ProjectLoadOptions::default())
            .unwrap()
            .discover(std::slice::from_ref(&root))
            .unwrap();
        let names: Vec<_> = paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.mjs", "b.cts", "z.ts"]);

        fs::remove_dir_all(root).unwrap();
    }
}
