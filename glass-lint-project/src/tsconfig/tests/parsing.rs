use super::*;
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
