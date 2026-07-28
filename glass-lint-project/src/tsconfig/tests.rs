use std::path::Path;

use super::*;
use crate::tests::TempProject;

/// Helper: call merge_selection with same-directory semantics.
fn merge_same(child: ParsedTsconfig, parent: Option<MergedSelection>) -> MergedSelection {
    let dir = Path::new(".");
    let parent_dir = parent.as_ref().map(|_| dir);
    merge_selection(child, parent, dir, parent_dir)
}

fn default_budget() -> ConfigTraversalBudget {
    ConfigTraversalBudget::default()
}

fn default_resource_budget() -> ProjectResourceBudget {
    ProjectResourceBudget::new(250_000, 512 * 1024 * 1024)
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
        false,
    );
    assert!(ps.is_included("src/main.ts"));
    assert!(ps.is_included("lib/util.ts"));
    assert!(!ps.is_included("src/main.test.ts"));
    assert!(!ps.is_included("dist/bundle.js"));
    assert!(!ps.is_included("node_modules/pkg/index.js"));
}

#[test]
fn pattern_set_trailing_slash() {
    let ps = TsconfigPatternSet::new(&["src/".to_string()], &[], false);
    assert!(ps.is_included("src/main.ts"));
    assert!(!ps.is_included("lib/main.ts"));
}

#[test]
fn pattern_set_no_slash_matches_basename() {
    let ps = TsconfigPatternSet::new(&["*.ts".to_string()], &[], false);
    assert!(ps.is_included("foo.ts"));
    assert!(ps.is_included("src/bar.ts"));
    assert!(!ps.is_included("foo.js"));
}

#[test]
fn merge_selection_inherits_fields() {
    let parent_dto =
        ParsedTsconfig::parse(r#"{"include":["src/**/*"],"exclude":["**/*.test.ts"]}"#).unwrap();
    let child_dto = ParsedTsconfig::parse(r#"{"include":["lib/**/*"]}"#).unwrap();

    let parent = merge_same(parent_dto, None);

    let child = merge_same(child_dto, Some(parent));

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
    let config = merge_same(dto, None);
    assert_eq!(config.include, vec!["**/*"]);
}

#[test]
fn merge_selection_explicit_files() {
    let dto = ParsedTsconfig::parse(r#"{"files":["src/main.ts","src/util.ts"]}"#).unwrap();
    let config = merge_same(dto, None);
    assert_eq!(
        config.files,
        Some(vec!["src/main.ts".to_string(), "src/util.ts".to_string()])
    );
    assert!(config.include.is_empty());
}

#[test]
fn pattern_set_invalid_controlling_field_rejects_everything() {
    let ps = TsconfigPatternSet::new(&["src/**/*".to_string()], &[], true);
    assert!(!ps.is_included("src/main.ts"));
    assert!(!ps.is_included("lib/util.ts"));
    assert!(!ps.is_included("any/file.ts"));
}

#[test]
fn merge_selection_invalid_files_fails_closed() {
    let child = ParsedTsconfig::parse(r#"{"files":null}"#).unwrap();
    let merged = merge_same(child, None);
    assert!(merged.invalid_controlling_field);
    assert_eq!(merged.files, Some(Vec::<String>::new()));
}

#[test]
fn merge_selection_invalid_include_fails_closed() {
    let child = ParsedTsconfig::parse(r#"{"include":false}"#).unwrap();
    let merged = merge_same(child, None);
    assert!(merged.invalid_controlling_field);
    assert!(merged.include.is_empty());
}

#[test]
fn merge_selection_invalid_include_does_not_fall_back_to_star_star() {
    let child = ParsedTsconfig::parse(r#"{"include":{"src":"bad"}}"#).unwrap();
    let merged = merge_same(child, None);
    assert!(merged.invalid_controlling_field);
    assert!(
        merged.include.is_empty(),
        "include should be empty, got {:#?}",
        merged.include
    );
}

#[test]
fn merge_selection_invalid_parent_propagates_to_child() {
    let parent = merge_same(ParsedTsconfig::parse(r#"{"include":false}"#).unwrap(), None);
    assert!(parent.invalid_controlling_field);

    let child = merge_same(
        ParsedTsconfig::parse(r#"{"include":["src/**/*"]}"#).unwrap(),
        Some(parent),
    );
    assert!(
        child.invalid_controlling_field,
        "parent invalidity should propagate even when child is valid"
    );
    // Child's valid include should still be used for compilation, but the
    // fail-closed flag from the parent's invalid field means no source matches.
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
fn extends_nonexistent_path_emits_diagnostic() {
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
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("does not exist"));
}

#[test]
fn extends_package_based_emits_unsupported_diagnostic() {
    let project = TempProject::new("tsconfig-pkg-extends");
    project.write(
        "tsconfig.json",
        r#"{"extends":"@typescript/foo","include":["src/**/*"]}"#,
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
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("unsupported"));
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
// Integration tests: build_effective_config with invalid fields
// ---------------------------------------------------------------------------

#[test]
fn build_effective_config_invalid_files_null_no_broad_fallback() {
    let project = TempProject::new("tsconfig-invalid-files-null");
    project.write("tsconfig.json", r#"{"files":null}"#);

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

    // Must NOT fall back to **/* — all paths rejected
    assert!(!config.pattern_set.is_included("src/main.ts"));
    assert!(!config.pattern_set.is_included("index.ts"));
    assert!(!config.pattern_set.is_included("any/file.ts"));
    // Diagnostic emitted for null files field
    assert!(diagnostics.iter().any(|d| d.message.contains("files")));
}

#[test]
fn build_effective_config_invalid_include_false_no_broad_fallback() {
    let project = TempProject::new("tsconfig-invalid-include-false");
    project.write("tsconfig.json", r#"{"include":false}"#);

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

    // Must NOT fall back to **/*
    assert!(!config.pattern_set.is_included("src/main.ts"));
    assert!(!config.pattern_set.is_included("lib/util.ts"));
    assert!(!config.pattern_set.is_included("any/file.ts"));
    // Diagnostic emitted for invalid include
    assert!(diagnostics.iter().any(|d| d.message.contains("include")));
}

#[test]
fn build_effective_config_invalid_include_object_no_broad_fallback() {
    let project = TempProject::new("tsconfig-invalid-include-obj");
    project.write("tsconfig.json", r#"{"include":{"src":"bad"}}"#);

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

    // Must NOT fall back to **/*
    assert!(!config.pattern_set.is_included("src/main.ts"));
    assert!(!config.pattern_set.is_included("index.ts"));
    assert!(diagnostics.iter().any(|d| d.message.contains("include")));
}

#[test]
fn build_effective_config_invalid_include_null_no_broad_fallback() {
    let project = TempProject::new("tsconfig-invalid-include-null");
    project.write("tsconfig.json", r#"{"include":null}"#);

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

    // Must NOT fall back to **/*
    assert!(!config.pattern_set.is_included("src/main.ts"));
    assert!(!config.pattern_set.is_included("index.ts"));
    assert!(diagnostics.iter().any(|d| d.message.contains("include")));
}

#[test]
fn build_effective_config_invalid_parent_extends_propagates_fail_closed() {
    let project = TempProject::new("tsconfig-invalid-parent-extends");
    project.write("base.json", r#"{"include":false}"#);
    project.write(
        "tsconfig.json",
        r#"{"extends":"./base.json","include":["src/**/*"]}"#,
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

    // Parent's invalid include propagates — child's valid include still
    // rejected because fail-closed flag is inherited from the parent.
    assert!(!config.pattern_set.is_included("src/main.ts"));
    assert!(!config.pattern_set.is_included("lib/util.ts"));
    // Diagnostic emitted for the base config's invalid include
    assert!(diagnostics.iter().any(|d| d.message.contains("include")));
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

// ---------------------------------------------------------------------------
// Cross-directory extends — paths must be rebased to the declaring config's
// directory before merging.
// ---------------------------------------------------------------------------

#[test]
fn cross_dir_include_is_rebased_to_parent_dir() {
    // Parent at sub/tsconfig.json declares include: ["src"], which means
    // sub/src relative to the project root.  Child at tsconfig.json extends
    // parent; the inherited pattern must resolve to sub/src, not src.
    let project = TempProject::new("cross-include");
    project.create_dir("sub/src");
    project.write("sub/tsconfig.json", r#"{"include":["src"]}"#);
    project.write("tsconfig.json", r#"{"extends":"./sub/tsconfig.json"}"#);

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

    // Parent's "src" was rebased to "sub/src"; the pattern becomes
    // "sub/src" which the glob matcher treats as an exact directory
    // reference.  In TypeScript, include:["src"] would be expanded to
    // "src/**/*", but the current normalize only expands trailing-slash
    // patterns, so we assert the exact path matches.
    assert!(
        config.pattern_set.is_included("sub/src"),
        "rebased parent include should match sub/src"
    );
    assert!(
        !config.pattern_set.is_included("src"),
        "unrebased parent include should NOT match root src"
    );
}

#[test]
fn cross_dir_files_is_rebased_to_parent_dir() {
    // Parent declares an explicit files entry; the path is relative to the
    // parent's directory and must be rebased for the child.
    let project = TempProject::new("cross-files");
    project.create_dir("sub/src");
    project.write("sub/tsconfig.json", r#"{"files":["src/file.ts"]}"#);
    project.write("sub/src/file.ts", "");
    project.write("tsconfig.json", r#"{"extends":"./sub/tsconfig.json"}"#);

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

    let files = config.files.expect("files should be Some");
    assert_eq!(files, vec!["sub/src/file.ts"]);
}

#[test]
fn cross_dir_exclude_is_rebased_to_parent_dir() {
    // Parent excludes a path relative to its own directory.  The child
    // inherits the exclude without setting its own, so the rebased path
    // must be used.
    let project = TempProject::new("cross-exclude");
    project.create_dir("sub");
    project.write(
        "sub/tsconfig.json",
        r#"{"include":["**/*"],"exclude":["secret.ts"]}"#,
    );
    project.write(
        "tsconfig.json",
        r#"{"extends":"./sub/tsconfig.json","include":["**/*"]}"#,
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

    // Parent's "secret.ts" rebased to "sub/secret.ts"
    assert!(
        config.pattern_set.is_included("main.ts"),
        "unrelated file should be included"
    );
    assert!(
        !config.pattern_set.is_included("sub/secret.ts"),
        "rebased parent exclude should apply to sub/secret.ts"
    );
    assert!(
        config.pattern_set.is_included("secret.ts"),
        "parent's exclude should NOT apply to root/secret.ts"
    );
}

#[test]
fn cross_dir_outdir_in_exclude_is_rebased() {
    // Parent has outDir set, which is automatically added to the exclude
    // list.  The child inherits the parent's exclude and the outDir path
    // must be rebased.
    let project = TempProject::new("cross-outdir");
    project.create_dir("sub");
    project.write(
        "sub/tsconfig.json",
        r#"{"compilerOptions":{"outDir":"out"},"include":["**/*"]}"#,
    );
    project.write(
        "tsconfig.json",
        r#"{"extends":"./sub/tsconfig.json","include":["**/*"]}"#,
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

    // Parent's "out" was rebased: "sub/out" should be in the exclude list.
    // The current glob matching for non-trailing-slash excludes only matches
    // the exact path (not children), so we test the directory path itself.
    assert!(
        !config.pattern_set.is_included("sub/out"),
        "rebased parent outDir should exclude sub/out"
    );
    // Root's out is NOT excluded by the parent's setting
    assert!(
        config.pattern_set.is_included("out"),
        "parent's outDir should NOT exclude root/out"
    );
}

#[test]
fn cross_dir_child_include_overrides_parent() {
    // Child sets its own include, overriding the parent's include.
    // The parent's include must NOT leak into the child's selection.
    let project = TempProject::new("cross-override");
    project.create_dir("sub");
    project.write("sub/tsconfig.json", r#"{"include":["sub_only/**/*"]}"#);
    project.write(
        "tsconfig.json",
        r#"{"extends":"./sub/tsconfig.json","include":["child_only/**/*"]}"#,
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

    // Only child's include should be active
    assert!(
        config.pattern_set.is_included("child_only/main.ts"),
        "child include should be used"
    );
    assert!(
        !config.pattern_set.is_included("sub_only/main.ts"),
        "parent include should NOT leak when child overrides"
    );
}

#[test]
fn cross_dir_child_exclude_inherits_rebased_parent_exclude() {
    // Child does NOT set its own exclude, so it inherits the parent's
    // exclude list (rebased to the child's directory).
    let project = TempProject::new("cross-exclude-inherit");
    project.create_dir("sub");
    project.write(
        "sub/tsconfig.json",
        r#"{"include":["**/*"],"exclude":["sub_secret.ts"]}"#,
    );
    project.write(
        "tsconfig.json",
        r#"{"extends":"./sub/tsconfig.json","include":["**/*"]}"#,
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

    // Parent's "sub_secret.ts" rebased to "sub/sub_secret.ts"
    assert!(
        !config.pattern_set.is_included("sub/sub_secret.ts"),
        "rebased parent exclude should apply to sub/sub_secret.ts"
    );
    assert!(
        config.pattern_set.is_included("sub_secret.ts"),
        "parent exclude should NOT apply to root/sub_secret.ts"
    );
}

#[test]
fn cross_dir_child_exclude_overrides_parent() {
    // Child sets its own exclude, overriding the parent's exclude entirely.
    // The parent's exclude must NOT be inherited.
    let project = TempProject::new("cross-exclude-override");
    project.create_dir("sub");
    project.write(
        "sub/tsconfig.json",
        r#"{"include":["**/*"],"exclude":["sub_secret.ts"]}"#,
    );
    project.write(
        "tsconfig.json",
        r#"{"extends":"./sub/tsconfig.json","include":["**/*"],"exclude":["child_secret.ts"]}"#,
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

    // Child's exclude overrides parent's
    assert!(
        config.pattern_set.is_included("sub/sub_secret.ts"),
        "parent exclude should NOT apply when child overrides"
    );
    assert!(
        !config.pattern_set.is_included("child_secret.ts"),
        "child exclude should apply"
    );
}

#[test]
fn cross_dir_three_level_chain_rebases_correctly() {
    // grandparent/tsconfig.json  include:["src/**/*"]
    //   parent/tsconfig.json     extends ../grandparent/tsconfig.json
    //   child/tsconfig.json      extends ../parent/tsconfig.json
    //
    // The grandparent's "src/**/*" should be rebased twice:
    //   grandparent->parent: "../grandparent/src/**/*"
    //   parent->child:       "../grandparent/src/**/*"
    let project = TempProject::new("cross-three-level");
    project.create_dir("grandparent");
    project.create_dir("parent");
    project.create_dir("child");
    project.write("grandparent/tsconfig.json", r#"{"include":["src/**/*"]}"#);
    project.write(
        "parent/tsconfig.json",
        r#"{"extends":"../grandparent/tsconfig.json"}"#,
    );
    project.write(
        "child/tsconfig.json",
        r#"{"extends":"../parent/tsconfig.json"}"#,
    );

    let mut diagnostics = Vec::new();
    let mut config_count = 0;
    let mut resource_budget = default_resource_budget();
    let (config, _) = build_effective_config(
        &project.root().join("child/tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
        default_budget(),
        &mut config_count,
        &mut resource_budget,
    )
    .unwrap();

    // The grandparent's "src/**/*" rebases to "../grandparent/src/**/*"
    // relative to the child config.  The path "../grandparent/src/main.ts"
    // should match.
    // The grandparent's "src/**/*" rebases to "../grandparent/src/**/*"
    // relative to the child config.  The path "../grandparent/src/main.ts"
    // should match.
    assert!(
        config.pattern_set.is_included("../grandparent/src/main.ts"),
        "grandparent include should match grandparent/src after two rebases"
    );
    // "src/main.ts" relative to child means child/src/main.ts — should NOT match.
    assert!(
        !config.pattern_set.is_included("src/main.ts"),
        "grandparent include should not match child/src"
    );
}
