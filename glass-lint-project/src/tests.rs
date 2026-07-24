//! Project-loader integration tests for discovery, budgets, resolution, and
//! deterministic phase metrics.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use glass_lint_core::{Environment, Linter, LinterConfig, RuleCatalog};

use crate::{
    ProjectLoadError, ProjectLoader, ProjectSelection, SourceCorpus,
    options::{ProjectLoadOptions, ValidatedProjectLoadOptions},
};

/// RAII temporary project directory that is created on construction and cleaned
/// up on drop (including on panic).  Shared across tests to avoid repeating the
/// same temp-directory setup/teardown and manual `remove_dir_all` calls.
pub struct TempProject {
    root: PathBuf,
}

static NEXT_TEMP_PROJECT: AtomicU64 = AtomicU64::new(0);

impl TempProject {
    pub fn new(label: &str) -> Self {
        let serial = NEXT_TEMP_PROJECT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-lint-project-{label}-{}-{serial}",
            std::process::id(),
        ));
        fs::create_dir(&root).unwrap();
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create_dir(&self, path: impl AsRef<Path>) {
        fs::create_dir_all(self.root.join(path)).unwrap();
    }

    pub fn write(&self, path: impl AsRef<Path>, content: impl AsRef<[u8]>) {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Use an empty catalog to isolate loader behavior from rule matching.
fn linter() -> Linter {
    Linter::new(LinterConfig::new(
        vec![RuleCatalog::new("test", vec![]).unwrap()],
        Environment::default(),
    ))
    .unwrap()
}

#[test]
fn directory_discovery_is_sorted_and_excludes_runtime_directories() {
    let project = TempProject::new("discovery");
    project.create_dir("src");
    project.create_dir("node_modules/pkg");
    project.write("src/z.ts", "");
    project.write("src/a.js", "");
    project.write("src/types.d.ts", "");
    project.write("src/types.d.cts", "");
    project.write("src/types.d.mts", "");
    project.write("node_modules/pkg/index.js", "");
    let loader = ProjectLoader::new(ProjectLoadOptions::default().validated().unwrap());
    let report = loader
        .load_and_lint(&linter(), &ProjectSelection::directory(project.root()))
        .unwrap();
    assert_eq!(
        report
            .report
            .files()
            .iter()
            .map(|file| file.path().as_str())
            .collect::<Vec<_>>(),
        ["src/a.js", "src/z.ts"]
    );
}

#[test]
fn resolver_suffix_options_are_validated_and_declarations_are_excluded() {
    let mut options = ProjectLoadOptions::default();
    options.extension_aliases.insert(".js".into(), vec![]);
    assert!(matches!(
        options.validated(),
        Err(ProjectLoadError::InvalidOptions(_))
    ));

    let mut options = ProjectLoadOptions::default();
    options.extensions.push(".d.cts".into());
    let _loader = ProjectLoader::new(options.validated().unwrap());
}

#[test]
fn discovery_stops_at_visited_entry_budget() {
    let project = TempProject::new("entries");
    project.create_dir("nested");
    project.write("nested/file.js", "");
    let options = ProjectLoadOptions {
        max_visited_entries: 1,
        ..Default::default()
    };
    let error = ProjectLoader::new(options.validated().unwrap())
        .load_and_lint(&linter(), &ProjectSelection::directory(project.root()))
        .unwrap_err();
    assert!(matches!(error, ProjectLoadError::TooManyEntries(1)));
}

#[test]
fn deterministic_loader_budget_returns_partial_report_and_error() {
    let project = TempProject::new("partial");
    project.write("a.js", "1234567890");
    project.write("b.js", "1234567890");
    let options = ProjectLoadOptions {
        max_project_source_bytes: 15,
        max_source_bytes: 10,
        ..Default::default()
    };
    let outcome = ProjectLoader::new(options.validated().unwrap())
        .load_and_lint(&linter(), &ProjectSelection::directory(project.root()))
        .unwrap();
    assert!(matches!(
        outcome.partial_reason,
        Some(ProjectLoadError::ProjectSourceTooLarge { .. })
    ));
    assert_eq!(
        outcome.report.completion(),
        glass_lint_core::project::ReportCompletion::Partial
    );
    assert_eq!(outcome.report.files().len(), 1);
    assert!(
        outcome
            .report
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == "incomplete_project")
    );
}

#[test]
fn extensionless_internal_import_is_followed() {
    let project = TempProject::new("ext");
    project.write("main.js", "import './helper';");
    project.write("helper.ts", "export const value = 1;");
    let loader = ProjectLoader::new(ProjectLoadOptions::default().validated().unwrap());
    let report = loader
        .load_and_lint(
            &linter(),
            &ProjectSelection::entry(project.root().join("main.js")),
        )
        .unwrap();
    assert_eq!(report.report.files().len(), 2);
}

#[test]
fn file_budget_deduplicates_shared_imports() {
    let project = TempProject::new("dedup");
    project.write("main.js", "import './a'; import './b';");
    project.write("a.js", "import './shared';");
    project.write("b.js", "import './shared';");
    project.write("shared.js", "export const x = 1;");
    let options = ProjectLoadOptions {
        max_files: 4,
        ..Default::default()
    };
    let outcome = ProjectLoader::new(options.validated().unwrap())
        .load_and_lint(
            &linter(),
            &ProjectSelection::entry(project.root().join("main.js")),
        )
        .unwrap();
    assert_eq!(outcome.report.files().len(), 4);
    assert_eq!(outcome.metrics.files, 4);
}

#[test]
fn file_budget_exhaustion_returns_error_at_limit() {
    let project = TempProject::new("exact-limit");
    project.write("a.js", "");
    project.write("b.js", "");
    let options = ProjectLoadOptions {
        max_files: 1,
        ..Default::default()
    };
    let error = ProjectLoader::new(options.validated().unwrap())
        .load_and_lint(&linter(), &ProjectSelection::directory(project.root()))
        .unwrap_err();
    assert!(matches!(error, ProjectLoadError::TooManyFiles(1)));
}

#[test]
fn reports_project_phase_metrics_and_operation_counts() {
    let project = TempProject::new("metrics");
    project.write("main.js", "import './helper';");
    project.write("helper.ts", "export const value = 1;");
    let loader = ProjectLoader::new(ProjectLoadOptions::default().validated().unwrap());
    let outcome = loader
        .load_and_lint(
            &linter(),
            &ProjectSelection::entry(project.root().join("main.js")),
        )
        .unwrap();
    assert_eq!(outcome.report.files().len(), 2);
    assert_eq!(outcome.metrics.files, 2);
    assert_eq!(outcome.metrics.requests, 1);
    assert_eq!(outcome.metrics.edges, 1);
    assert!(outcome.metrics.timings.total() >= outcome.metrics.timings.linking_and_matching());
}

#[test]
fn tsconfig_membership_accepts_jsonc_and_excludes_files() {
    let project = TempProject::new("tsconfig");
    project.create_dir("src");
    project.write("src/main.ts", "export const main = 1;");
    project.write("src/test.ts", "export const test = 1;");
    project.write(
        "tsconfig.json",
        "{\n  // runtime project\n  \"include\": [\"src/**/*.ts\",],\n  \"exclude\": [\"src/test.ts\",],\n}",
    );
    let loader = ProjectLoader::new(ProjectLoadOptions::default().validated().unwrap());
    let report = loader
        .load_and_lint(
            &linter(),
            &ProjectSelection::tsconfig(project.root().join("tsconfig.json")),
        )
        .unwrap();
    assert_eq!(
        report
            .report
            .files()
            .iter()
            .map(|file| file.path().as_str())
            .collect::<Vec<_>>(),
        ["src/main.ts"]
    );
}

#[test]
fn tsconfig_membership_inherits_extends_and_collects_references() {
    let project = TempProject::new("tsconfig-inherited");
    project.create_dir("src");
    project.create_dir("generated");
    project.create_dir("packages/child/src");
    project.write("src/main.ts", "export const main = 1;");
    project.write("generated/main.ts", "export const generated = 1;");
    project.write("packages/child/src/value.ts", "export const value = 1;");
    project.write(
        "base.json",
        "{\"include\":[\"src/**/*.ts\"],\"compilerOptions\":{\"outDir\":\"generated\"}}",
    );
    project.write(
        "packages/child/tsconfig.json",
        "{\"include\":[\"src/**/*.ts\"]}",
    );
    project.write(
        "tsconfig.json",
        "{\"extends\":\"./base.json\",\"references\":[{\"path\":\"packages/child\"}]}",
    );

    let loader = ProjectLoader::new(ProjectLoadOptions::default().validated().unwrap());
    let report = loader
        .load_and_lint(
            &linter(),
            &ProjectSelection::tsconfig(project.root().join("tsconfig.json")),
        )
        .unwrap();
    assert_eq!(
        report
            .report
            .files()
            .iter()
            .map(|file| file.path().as_str())
            .collect::<Vec<_>>(),
        ["packages/child/src/value.ts", "src/main.ts"]
    );
}

#[test]
fn invalid_configured_root_returns_error_not_fallback() {
    let result = SourceCorpus::from_validated(
        &ValidatedProjectLoadOptions::builder()
            .root("/glass-lint-test-nonexistent-root-does-not-exist")
            .build()
            .unwrap(),
    );
    assert!(
        result.is_err(),
        "a non-existent configured root must return an Err, not a fallback"
    );
}
