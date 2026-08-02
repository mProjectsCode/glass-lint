use super::*;
#[test]
fn merge_selection_inherits_fields() {
    let parent_dto =
        ParsedTsconfig::parse(r#"{"include":["src/**/*"],"exclude":["**/*.test.ts"]}"#).unwrap();
    let child_dto = ParsedTsconfig::parse(r#"{"include":["lib/**/*"]}"#).unwrap();

    let parent = merge_same(parent_dto, None);

    let child = merge_same(child_dto, Some(parent));

    // Child include overrides parent
    assert_eq!(child.include(), ["lib/**/*"]);
    // Exclude is inherited (child didn't set it)
    assert!(child.exclude().iter().any(|e| e == "**/*.test.ts"));
    // Default node_modules exclusion
    assert!(child.exclude().iter().any(|e| e == "**/node_modules"));
}

#[test]
fn merge_selection_default_include() {
    let dto = ParsedTsconfig::parse("{}").unwrap();
    let config = merge_same(dto, None);
    assert_eq!(config.include(), ["**/*"]);
}

#[test]
fn merge_selection_explicit_files() {
    let dto = ParsedTsconfig::parse(r#"{"files":["src/main.ts","src/util.ts"]}"#).unwrap();
    let config = merge_same(dto, None);
    assert_eq!(
        config.files(),
        Some(["src/main.ts".to_string(), "src/util.ts".to_string()].as_slice())
    );
    assert!(config.include().is_empty());
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
    assert!(merged.invalid_controlling_field());
    assert_eq!(merged.files(), Some(Vec::<String>::new().as_slice()));
}

#[test]
fn merge_selection_invalid_include_fails_closed() {
    let child = ParsedTsconfig::parse(r#"{"include":false}"#).unwrap();
    let merged = merge_same(child, None);
    assert!(merged.invalid_controlling_field());
    assert!(merged.include().is_empty());
}

#[test]
fn merge_selection_invalid_include_does_not_fall_back_to_star_star() {
    let child = ParsedTsconfig::parse(r#"{"include":{"src":"bad"}}"#).unwrap();
    let merged = merge_same(child, None);
    assert!(merged.invalid_controlling_field());
    assert!(
        merged.include().is_empty(),
        "include should be empty, got {:#?}",
        merged.include()
    );
}

#[test]
fn merge_selection_invalid_parent_propagates_to_child() {
    let parent = merge_same(ParsedTsconfig::parse(r#"{"include":false}"#).unwrap(), None);

    let child = merge_same(
        ParsedTsconfig::parse(r#"{"include":["src/**/*"]}"#).unwrap(),
        Some(parent),
    );
    assert!(
        child.invalid_controlling_field(),
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
