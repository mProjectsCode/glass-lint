use std::time::{Duration, Instant};

use super::*;
use crate::tests::TempProject;

fn default_budget() -> ConfigTraversalBudget {
    ConfigTraversalBudget::default()
}

fn default_resource_budget() -> ProjectResourceBudget {
    ProjectResourceBudget::new(
        250_000,
        512 * 1024 * 1024,
        Instant::now() + Duration::from_secs(3600),
    )
}

#[test]
fn parse_empty_config() {
    let dto = ParsedTsconfig::parse("{}").unwrap();
    assert!(matches!(dto.extends, StringField::Absent));
    assert!(matches!(dto.files, StringArrayField::Absent));
    assert!(matches!(dto.include, StringArrayField::Absent));
    assert!(matches!(dto.exclude, StringArrayField::Absent));
    assert!(dto.references.is_empty());
}

#[test]
fn parse_null_fields() {
    let dto =
        ParsedTsconfig::parse(r#"{"extends":null,"files":null,"include":null,"exclude":null}"#)
            .unwrap();
    assert!(matches!(dto.extends, StringField::Null));
    assert!(matches!(dto.files, StringArrayField::Null));
    assert!(matches!(dto.include, StringArrayField::Null));
    assert!(matches!(dto.exclude, StringArrayField::Null));
}

#[test]
fn parse_wrong_types() {
    let dto = ParsedTsconfig::parse(
        r#"{"extends":42,"files":"not-an-array","include":false,"exclude":{}}"#,
    )
    .unwrap();
    assert!(matches!(&dto.extends, StringField::WrongType(_)));
    assert!(matches!(&dto.files, StringArrayField::WrongType(_)));
    assert!(matches!(&dto.include, StringArrayField::WrongType(_)));
    assert!(matches!(&dto.exclude, StringArrayField::WrongType(_)));
}

#[test]
fn parse_compiler_options() {
    let dto =
        ParsedTsconfig::parse(r#"{"compilerOptions":{"outDir":"dist","declarationDir":"types"}}"#)
            .unwrap();
    assert_eq!(dto.compiler_options_out_dir.ok(), Some("dist".into()));
    assert_eq!(
        dto.compiler_options_declaration_dir.ok(),
        Some("types".into())
    );
}

#[test]
fn parse_references() {
    let dto =
        ParsedTsconfig::parse(r#"{"references":[{"path":"./child"},{"path":"./other"}]}"#).unwrap();
    assert_eq!(
        dto.references,
        vec![
            ReferenceEntry {
                path: "./child".into()
            },
            ReferenceEntry {
                path: "./other".into()
            }
        ]
    );
}

#[test]
fn parse_jsonc() {
    let mut text = "{\n  // comment\n  \"include\": [\"src\"],\n}".to_string();
    json_strip_comments::strip(&mut text).unwrap();
    let dto = ParsedTsconfig::parse(&text).unwrap();
    assert!(matches!(&dto.include, StringArrayField::Present(v) if v == &["src"]));
}

#[test]
fn pattern_set_compilation_and_matching() {
    let ps = TsconfigPatternSet::new(
        &["src/**/*".to_string(), "lib/**/*".to_string()],
        &["**/*.test.ts".to_string()],
    );
    assert!(ps.is_included("src/main.ts"));
    assert!(ps.is_included("lib/util.ts"));
    assert!(!ps.is_included("src/main.test.ts"));
    assert!(!ps.is_included("dist/bundle.js"));
    assert!(!ps.is_included("node_modules/pkg/index.js"));
}

#[test]
fn pattern_set_trailing_slash() {
    let ps = TsconfigPatternSet::new(&["src/".to_string()], &[]);
    assert!(ps.is_included("src/main.ts"));
    assert!(!ps.is_included("lib/main.ts"));
}

#[test]
fn pattern_set_no_slash_matches_basename() {
    let ps = TsconfigPatternSet::new(&["*.ts".to_string()], &[]);
    assert!(ps.is_included("foo.ts"));
    assert!(ps.is_included("src/bar.ts"));
    assert!(!ps.is_included("foo.js"));
}

#[test]
fn merge_selection_inherits_fields() {
    let parent_dto =
        ParsedTsconfig::parse(r#"{"include":["src/**/*"],"exclude":["**/*.test.ts"]}"#).unwrap();
    let child_dto = ParsedTsconfig::parse(r#"{"include":["lib/**/*"]}"#).unwrap();

    let parent = merge_selection(parent_dto, None);

    let child = merge_selection(child_dto, Some(parent));

    // Child include overrides parent
    assert_eq!(child.include, vec!["lib/**/*"]);
    // Exclude is inherited (child didn't set it)
    assert!(child.exclude.iter().any(|e| e == "**/*.test.ts"));
    // Default node_modules exclusion
    assert!(child.exclude.iter().any(|e| e == "**/node_modules"));
}

#[test]
fn merge_selection_default_include() {
    let dto = ParsedTsconfig::parse("{}").unwrap();
    let config = merge_selection(dto, None);
    assert_eq!(config.include, vec!["**/*"]);
}

#[test]
fn merge_selection_explicit_files() {
    let dto = ParsedTsconfig::parse(r#"{"files":["src/main.ts","src/util.ts"]}"#).unwrap();
    let config = merge_selection(dto, None);
    assert_eq!(
        config.files,
        Some(vec!["src/main.ts".to_string(), "src/util.ts".to_string()])
    );
    assert!(config.include.is_empty());
}

#[test]
fn cycle_detection_records_diagnostic_and_skips_cyclic_extends() {
    let project = TempProject::new("tsconfig-cycle");
    project.write(
        "tsconfig.json",
        r#"{"extends":"./tsconfig.json","include":["src/**/*"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let config_path = project.root().join("tsconfig.json");
    let result = build_effective_config(
        &config_path,
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    );

    assert!(
        result.is_ok(),
        "build_effective_config failed: {:?}",
        result.err()
    );
    let (config, _references) = result.unwrap();
    // Cycle extends is skipped; config uses its own include
    assert_eq!(config.files, None);
    assert!(config.pattern_set.is_included("src/main.ts"));
    assert!(!config.pattern_set.is_included("other/file.ts"));
    // Cycle diagnostics recorded
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("cycle"));
}

#[test]
fn cycle_fails_closed_does_not_broaden_admission() {
    // Create config A that extends B, and B that extends A (cycle)
    let project = TempProject::new("tsconfig-cycle2");
    project.write("a.json", r#"{"extends":"./b.json","include":["src/**/*"]}"#);
    project.write(
        "b.json",
        r#"{"extends":"./a.json","include":["other/**/*"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();

    // Build effective config for A
    let result = build_effective_config(
        &project.root().join("a.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    );

    assert!(result.is_ok());
    let (config, _) = result.unwrap();
    // A should have include: ["src/**/*"] (its own setting)
    // The cycle in extends should NOT bring in B's patterns
    assert!(config.files.is_none(), "no explicit files");
    assert!(
        config.pattern_set.is_included("src/main.ts"),
        "A's include should be used"
    );
    assert!(
        !config.pattern_set.is_included("other/bar.ts"),
        "B's include should not be inherited through cycle"
    );
    // Cycle diagnostic recorded for the B->A link
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("cycle"));
}

#[test]
fn missing_config_field_returns_typed_diagnostic() {
    // Parsing a config with wrong types should succeed (we record diagnostics
    // as typed fields, not errors)
    let dto = ParsedTsconfig::parse(r#"{"include":123,"exclude":null}"#).unwrap();
    assert!(matches!(&dto.include, StringArrayField::WrongType(_)));
    assert!(matches!(&dto.exclude, StringArrayField::Null));
}

#[test]
fn extends_nonexistent_path_is_skipped_silently() {
    let project = TempProject::new("tsconfig-missing-extends");
    project.write(
        "tsconfig.json",
        r#"{"extends":"./nonexistent.json","include":["src/**/*"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    assert!(config.pattern_set.is_included("src/main.ts"));
    assert!(diagnostics.is_empty());
}

#[test]
fn single_level_extends_merges_correctly() {
    let project = TempProject::new("tsconfig-merge");
    project.write(
        "base.json",
        r#"{"include":["src/**/*"],"exclude":["**/*.test.ts"]}"#,
    );
    project.write(
        "tsconfig.json",
        r#"{"extends":"./base.json","exclude":["**/*.spec.ts"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    // Child exclude replaces parent exclude since child sets its own.
    // Parent's exclude ("**/*.test.ts") should NOT be inherited.
    // The compiled pattern set should reflect child's exclude.
    assert!(
        config.pattern_set.is_included("src/main.test.ts"),
        "parent exclude not inherited when child sets its own"
    );
    assert!(
        !config.pattern_set.is_included("src/main.spec.ts"),
        "child exclude should apply"
    );
    // Default exclusions still apply
    assert!(
        !config.pattern_set.is_included("node_modules/pkg/index.js"),
        "default node_modules exclusion applies"
    );
}

// ---------------------------------------------------------------------------
// ConfigTraversalBudget tests
// ---------------------------------------------------------------------------

#[test]
fn extends_within_budget_succeeds() {
    let project = TempProject::new("budget-within-extends");
    project.write("base.json", r#"{"include":["src/**/*"]}"#);
    project.write(
        "tsconfig.json",
        r#"{"extends":"./base.json","include":["lib/**/*"]}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let budget = ConfigTraversalBudget::new(10, 5);
    let result = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        budget,
        &mut config_count,
        &mut resource_budget,
    );

    assert!(result.is_ok(), "within-budget extends should succeed");
}

#[test]
fn extends_exceeding_max_depth_fails() {
    let project = TempProject::new("budget-depth-extends");
    // Chain: a -> b -> c with max_depth=2 should fail
    project.write("c.json", r#"{"include":["c/**/*"]}"#);
    project.write("b.json", r#"{"extends":"./c.json","include":["b/**/*"]}"#);
    project.write("a.json", r#"{"extends":"./b.json","include":["a/**/*"]}"#);

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    // max_depth=2 allows root + one extends but not root + two extends
    let budget = ConfigTraversalBudget::new(10, 2);
    let err = build_effective_config(
        &project.root().join("a.json"),
        project.root(),
        None,
        &mut diagnostics,
        budget,
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            ProjectLoadError::ConfigBudgetExhausted {
                kind: "extends depth",
                ..
            }
        ),
        "expected extends depth error, got {err:?}"
    );
}

#[test]
fn extends_exceeding_max_config_count_fails() {
    let project = TempProject::new("budget-count-extends");
    // Chain: a -> b -> c with max_config_count=2 should fail (3 configs)
    project.write("c.json", r#"{"include":["c/**/*"]}"#);
    project.write("b.json", r#"{"extends":"./c.json","include":["b/**/*"]}"#);
    project.write("a.json", r#"{"extends":"./b.json","include":["a/**/*"]}"#);

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let budget = ConfigTraversalBudget::new(2, 10);
    let err = build_effective_config(
        &project.root().join("a.json"),
        project.root(),
        None,
        &mut diagnostics,
        budget,
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            ProjectLoadError::ConfigBudgetExhausted {
                kind: "config count",
                ..
            }
        ),
        "expected config count error, got {err:?}"
    );
}

#[test]
fn extends_at_max_config_count_succeeds() {
    let project = TempProject::new("budget-count-at");
    // Chain: a -> b with max_config_count=2 should succeed (2 configs)
    project.write("b.json", r#"{"include":["b/**/*"]}"#);
    project.write("a.json", r#"{"extends":"./b.json","include":["a/**/*"]}"#);

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let budget = ConfigTraversalBudget::new(2, 10);
    let result = build_effective_config(
        &project.root().join("a.json"),
        project.root(),
        None,
        &mut diagnostics,
        budget,
        &mut config_count,
        &mut resource_budget,
    );

    assert!(result.is_ok(), "at-limit extends should succeed");
}

#[test]
fn extends_at_max_depth_succeeds() {
    let project = TempProject::new("budget-depth-at");
    // Chain: a -> b with max_depth=2 should succeed (depth: root=a, then b)
    project.write("b.json", r#"{"include":["b/**/*"]}"#);
    project.write("a.json", r#"{"extends":"./b.json","include":["a/**/*"]}"#);

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let budget = ConfigTraversalBudget::new(10, 2);
    let result = build_effective_config(
        &project.root().join("a.json"),
        project.root(),
        None,
        &mut diagnostics,
        budget,
        &mut config_count,
        &mut resource_budget,
    );

    assert!(result.is_ok(), "at-limit depth extends should succeed");
}
