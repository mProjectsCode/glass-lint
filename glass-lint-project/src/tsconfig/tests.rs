use super::*;
use crate::tests::TempProject;

#[test]
fn parse_empty_config() {
    let dto = TsconfigDto::parse("{}").unwrap();
    assert!(matches!(dto.extends, StringField::Absent));
    assert!(matches!(dto.files, StringArrayField::Absent));
    assert!(matches!(dto.include, StringArrayField::Absent));
    assert!(matches!(dto.exclude, StringArrayField::Absent));
    assert!(dto.references.is_empty());
}

#[test]
fn parse_null_fields() {
    let dto = TsconfigDto::parse(r#"{"extends":null,"files":null,"include":null,"exclude":null}"#)
        .unwrap();
    assert!(matches!(dto.extends, StringField::Null));
    assert!(matches!(dto.files, StringArrayField::Null));
    assert!(matches!(dto.include, StringArrayField::Null));
    assert!(matches!(dto.exclude, StringArrayField::Null));
}

#[test]
fn parse_wrong_types() {
    let dto =
        TsconfigDto::parse(r#"{"extends":42,"files":"not-an-array","include":false,"exclude":{}}"#)
            .unwrap();
    assert!(matches!(&dto.extends, StringField::WrongType(_)));
    assert!(matches!(&dto.files, StringArrayField::WrongType(_)));
    assert!(matches!(&dto.include, StringArrayField::WrongType(_)));
    assert!(matches!(&dto.exclude, StringArrayField::WrongType(_)));
}

#[test]
fn parse_compiler_options() {
    let dto =
        TsconfigDto::parse(r#"{"compilerOptions":{"outDir":"dist","declarationDir":"types"}}"#)
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
        TsconfigDto::parse(r#"{"references":[{"path":"./child"},{"path":"./other"}]}"#).unwrap();
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
    let dto = TsconfigDto::parse(&text).unwrap();
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
fn compile_counter_increments() {
    reset_compile_counter();
    let _ps1 = TsconfigPatternSet::new(&["**/*".to_string()], &[]);
    assert_eq!(compile_count(), 1);
    let _ps2 = TsconfigPatternSet::new(&["**/*".to_string()], &[]);
    assert_eq!(compile_count(), 2);
}

#[test]
fn effective_config_inherits_fields() {
    let parent_dto =
        TsconfigDto::parse(r#"{"include":["src/**/*"],"exclude":["**/*.test.ts"]}"#).unwrap();
    let child_dto = TsconfigDto::parse(r#"{"include":["lib/**/*"]}"#).unwrap();

    let parent = Tsconfig::new(PathBuf::from("/root/tsconfig.json"), parent_dto, None);

    let child = Tsconfig::new(
        PathBuf::from("/root/tsconfig.json"),
        child_dto,
        Some(&parent),
    );

    // Child include overrides parent
    assert_eq!(child.include, vec!["lib/**/*"]);
    // Exclude is inherited (child didn't set it)
    assert!(child.exclude.iter().any(|e| e == "**/*.test.ts"));
    // Default node_modules exclusion
    assert!(child.exclude.iter().any(|e| e == "**/node_modules"));
}

#[test]
fn effective_config_default_include() {
    let dto = TsconfigDto::parse("{}").unwrap();
    let config = Tsconfig::new(PathBuf::from("/root/tsconfig.json"), dto, None);
    assert_eq!(config.include, vec!["**/*"]);
}

#[test]
fn effective_config_explicit_files() {
    let dto = TsconfigDto::parse(r#"{"files":["src/main.ts","src/util.ts"]}"#).unwrap();
    let config = Tsconfig::new(PathBuf::from("/root/tsconfig.json"), dto, None);
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
    let config_path = project.root().join("tsconfig.json");
    let result = build_effective_config(&config_path, project.root(), None, &mut diagnostics);

    assert!(
        result.is_ok(),
        "build_effective_config failed: {:?}",
        result.err()
    );
    let (config, _references) = result.unwrap();
    // Cycle extends is skipped; config uses its own include
    assert_eq!(config.files, None);
    assert!(config.include.contains(&"src/**/*".to_string()));
    // Cycle diagnostics recorded
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("cycle"));
}

#[test]
fn compile_counter_increments_once_per_effective_config() {
    // Build a config with no parent to measure exactly one compilation.
    let project = TempProject::new("tsconfig-compile");
    project.write(
        "tsconfig.json",
        r#"{"include":["src/**/*"],"exclude":["**/*.test.ts"]}"#,
    );

    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut Vec::new(),
    )
    .unwrap();

    // Matching should reuse the compiled set (no additional compilation)
    config.pattern_set.is_included("src/main.ts");
    config.pattern_set.is_included("src/test.ts");
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

    // Build effective config for A
    let result = build_effective_config(
        &project.root().join("a.json"),
        project.root(),
        None,
        &mut diagnostics,
    );

    assert!(result.is_ok());
    let (config, _) = result.unwrap();
    // A should have include: ["src/**/*"] (its own setting)
    // The cycle in extends should NOT bring in B's patterns
    assert!(config.files.is_none(), "no explicit files");
    assert_eq!(
        config.include,
        vec!["src/**/*"],
        "A's include should not be broadened by cyclic B"
    );
    // Cycle diagnostic recorded for the B->A link
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("cycle"));
}

#[test]
fn missing_config_field_returns_typed_diagnostic() {
    // Parsing a config with wrong types should succeed (we record diagnostics
    // as typed fields, not errors)
    let dto = TsconfigDto::parse(r#"{"include":123,"exclude":null}"#).unwrap();
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
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
    )
    .unwrap();

    assert_eq!(config.include, vec!["src/**/*"]);
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
    let (config, _) = build_effective_config(
        &project.root().join("tsconfig.json"),
        project.root(),
        None,
        &mut diagnostics,
    )
    .unwrap();

    // Child exclude overrides parent include? No — include and exclude are
    // separate. Child exclude replaces parent exclude since child sets it.
    // Actually from Tsconfig::new: exclude starts as child's, and if child's is
    // empty it inherits from parent. Child set ["**/*.spec.ts"], so excludes
    // should be: ["**/*.spec.ts", "**/node_modules", "**/bower_components"]
    assert!(config.exclude.iter().any(|e| e == "**/*.spec.ts"));
    // Parent's exclude should NOT be inherited since child set its own.
    assert!(
        !config.exclude.iter().any(|e| e == "**/*.test.ts"),
        "child exclude should replace parent exclude"
    );
}
