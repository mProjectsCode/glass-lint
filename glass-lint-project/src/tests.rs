use std::fs;

use glass_lint_core::{Environment, Linter, RuleCatalog};

use crate::{ProjectLoadError, ProjectLoadOptions, ProjectLoader, ProjectSelection};

fn linter() -> Linter {
    Linter::new(RuleCatalog::with_environment("test", vec![], Environment::default()).unwrap())
}

#[test]
fn directory_discovery_is_sorted_and_excludes_runtime_directories() {
    let root = std::env::temp_dir().join(format!("glass-lint-project-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
    fs::write(root.join("src/z.ts"), "").unwrap();
    fs::write(root.join("src/a.js"), "").unwrap();
    fs::write(root.join("src/types.d.ts"), "").unwrap();
    fs::write(root.join("src/types.d.cts"), "").unwrap();
    fs::write(root.join("src/types.d.mts"), "").unwrap();
    fs::write(root.join("node_modules/pkg/index.js"), "").unwrap();
    let loader = ProjectLoader::new(ProjectLoadOptions::default()).unwrap();
    let report = loader
        .load_and_lint(&linter(), &ProjectSelection::directory(&root))
        .unwrap();
    assert_eq!(
        report
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["src/a.js", "src/z.ts"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resolver_suffix_options_are_validated_and_declarations_are_excluded() {
    let mut options = ProjectLoadOptions::default();
    options.extension_aliases.insert(".js".into(), vec![]);
    assert!(matches!(
        ProjectLoader::new(options),
        Err(ProjectLoadError::InvalidOptions(_))
    ));

    let mut options = ProjectLoadOptions::default();
    options.extensions.push(".d.cts".into());
    assert!(ProjectLoader::new(options).is_ok());
}

#[test]
fn extensionless_internal_import_is_followed() {
    let root = std::env::temp_dir().join(format!("glass-lint-project-ext-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("main.js"), "import './helper';").unwrap();
    fs::write(root.join("helper.ts"), "export const value = 1;").unwrap();
    let loader = ProjectLoader::new(ProjectLoadOptions::default()).unwrap();
    let report = loader
        .load_and_lint(&linter(), &ProjectSelection::entry(root.join("main.js")))
        .unwrap();
    assert_eq!(report.files.len(), 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reports_project_phase_metrics_and_operation_counts() {
    let root =
        std::env::temp_dir().join(format!("glass-lint-project-metrics-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("main.js"), "import './helper';").unwrap();
    fs::write(root.join("helper.ts"), "export const value = 1;").unwrap();
    let loader = ProjectLoader::new(ProjectLoadOptions::default()).unwrap();
    let (report, metrics) = loader
        .load_and_lint_with_metrics(&linter(), &ProjectSelection::entry(root.join("main.js")))
        .unwrap();
    assert_eq!(report.files.len(), 2);
    assert_eq!(metrics.files, 2);
    assert_eq!(metrics.requests, 1);
    assert_eq!(metrics.edges, 1);
    assert!(metrics.total >= metrics.linking_and_matching);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tsconfig_membership_accepts_jsonc_and_excludes_files() {
    let root = std::env::temp_dir().join(format!(
        "glass-lint-project-tsconfig-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.ts"), "export const main = 1;").unwrap();
    fs::write(root.join("src/test.ts"), "export const test = 1;").unwrap();
    fs::write(
        root.join("tsconfig.json"),
        "{\n  // runtime project\n  \"include\": [\"src/**/*.ts\",],\n  \"exclude\": [\"src/test.ts\",],\n}",
    )
    .unwrap();
    let loader = ProjectLoader::new(ProjectLoadOptions::default()).unwrap();
    let report = loader
        .load_and_lint(
            &linter(),
            &ProjectSelection::tsconfig(root.join("tsconfig.json")),
        )
        .unwrap();
    assert_eq!(
        report
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["src/main.ts"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tsconfig_membership_inherits_extends_and_collects_references() {
    let root = std::env::temp_dir().join(format!(
        "glass-lint-project-tsconfig-inherited-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("generated")).unwrap();
    fs::create_dir_all(root.join("packages/child/src")).unwrap();
    fs::write(root.join("src/main.ts"), "export const main = 1;").unwrap();
    fs::write(
        root.join("generated/main.ts"),
        "export const generated = 1;",
    )
    .unwrap();
    fs::write(
        root.join("packages/child/src/value.ts"),
        "export const value = 1;",
    )
    .unwrap();
    fs::write(
        root.join("base.json"),
        "{\"include\":[\"src/**/*.ts\"],\"compilerOptions\":{\"outDir\":\"generated\"}}",
    )
    .unwrap();
    fs::write(
        root.join("packages/child/tsconfig.json"),
        "{\"include\":[\"src/**/*.ts\"]}",
    )
    .unwrap();
    fs::write(
        root.join("tsconfig.json"),
        "{\"extends\":\"./base.json\",\"references\":[{\"path\":\"packages/child\"}]}",
    )
    .unwrap();

    let loader = ProjectLoader::new(ProjectLoadOptions::default()).unwrap();
    let report = loader
        .load_and_lint(
            &linter(),
            &ProjectSelection::tsconfig(root.join("tsconfig.json")),
        )
        .unwrap();
    assert_eq!(
        report
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["packages/child/src/value.ts", "src/main.ts"]
    );
    fs::remove_dir_all(root).unwrap();
}
