//! Filesystem discovery and per-file lint execution.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use glass_lint_core::Linter;
use glass_lint_project::{ProjectLoader, ProjectSelection, SourceCorpus};

use crate::{
    args::Command,
    config::{self, Config},
    output::FileOutput,
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
            crate::output::write_mode(config, "single file", &path)?;
            let options = config.project_load_options()?;
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
    let selection = project_selection(path);
    let (mode, mode_path) = match &selection {
        ProjectSelection::Entry(path) => ("single file", path.as_path()),
        ProjectSelection::Directory(path) => ("folder", path.as_path()),
        ProjectSelection::Tsconfig(path) => ("tsconfig", path.as_path()),
    };
    crate::output::write_mode(config, mode, mode_path)?;
    let options = config.project_load_options()?;
    let loader = ProjectLoader::new(options);
    let outcome = loader
        .load_and_lint(linter, &selection)
        .with_context(|| format!("analyze project at {}", path.display()))?;
    let report = outcome.report;
    let failed = outcome.partial_reason.is_some() || config.report_fails(&report);
    crate::output::write_project_report(config, &report)?;
    tracing::info!(target: "glass_lint::cli", files = report.files.len(), "project command completed");
    Ok(failed)
}

fn project_selection(path: &std::path::Path) -> ProjectSelection {
    if path.is_dir() {
        let tsconfig = path.join("tsconfig.json");
        if tsconfig.is_file() {
            return ProjectSelection::tsconfig(tsconfig);
        }
        ProjectSelection::directory(path.to_path_buf())
    } else if path.file_name().is_some_and(|name| name == "tsconfig.json") {
        ProjectSelection::tsconfig(path.to_path_buf())
    } else {
        ProjectSelection::entry(path.to_path_buf())
    }
}

fn lint_files(config: &Config, linter: &Linter, paths: Vec<PathBuf>) -> Result<bool> {
    let options = config.project_load_options()?;
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

        let project_report = linter
            .lint_snippet(&source, &name)
            .map_err(|error| anyhow::anyhow!(error))?;
        failed |= config.report_fails(&project_report);
        files.push(FileOutput {
            path: name,
            report: project_report,
            source,
        });
    }

    crate::output::write_report(config, &files)?;
    tracing::info!(target: "glass_lint::cli", files = files.len(), "command completed");
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use glass_lint_project::{ProjectLoadOptions, ProjectSelection, SourceCorpus};

    use super::project_selection;

    #[test]
    fn directory_selection_prefers_local_tsconfig() {
        let root =
            std::env::temp_dir().join(format!("glass-lint-cli-selection-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();

        assert_eq!(
            project_selection(&root),
            ProjectSelection::Directory(root.clone())
        );

        let tsconfig = root.join("tsconfig.json");
        fs::write(&tsconfig, "{}").unwrap();
        assert_eq!(
            project_selection(&root),
            ProjectSelection::Tsconfig(tsconfig)
        );

        fs::remove_dir_all(root).unwrap();
    }

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
