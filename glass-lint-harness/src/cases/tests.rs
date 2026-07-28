use super::*;

#[test]
fn parses_comment_case() {
    let source = "\
// @case description Dynamic code
// @tool glass-lint rules=js:dynamic-code.string-timer
// @expect-error glass-lint rule=js:dynamic-code.string-timer
globalThis.setTimeout('run()', 10);
";
    let case = parse_case(
        Path::new("fixtures"),
        Path::new("fixtures/system/timer.js"),
        source.into(),
    )
    .unwrap();
    assert_eq!(case.id, "system/timer");
    assert_eq!(case.description, "Dynamic code");
    assert_eq!(case.adapters["glass-lint"].required()[0].line, Some(4));
}

#[test]
fn parses_certainty_expectations() {
    let source = "\
// @tool glass-lint rules=js:network.request
// @expect-error glass-lint rule=js:network.request certainty=possible
fetch('/remote');
";
    let case = parse_case(
        Path::new("fixtures"),
        Path::new("fixtures/network/possible.js"),
        source.into(),
    )
    .unwrap();
    assert_eq!(
        case.adapters["glass-lint"].required()[0].certainty,
        Some(glass_lint_core::MatchCertainty::Possible)
    );
}

#[test]
fn rejects_unknown_certainty_expectations() {
    let source = "\
// @tool glass-lint rules=js:network.request
// @expect-error glass-lint rule=js:network.request certainty=maybe
fetch('/remote');
";
    let error = parse_case(
        Path::new("fixtures"),
        Path::new("fixtures/network/possible.js"),
        source.into(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("unknown certainty"));
}

#[test]
fn parses_forbidden_diagnostic() {
    let source = "\
// @tool glass-lint rules=js:network.request
fetch('/remote'); // @expect-error glass-lint rule=js:network.request
function local(fetch) { fetch('/local'); } // @expect-no-error glass-lint rule=js:network.request
";
    let case = parse_case(
        Path::new("fixtures"),
        Path::new("fixtures/network/precision.js"),
        source.into(),
    )
    .unwrap();

    assert_eq!(case.adapters["glass-lint"].forbidden().len(), 1);
    assert_eq!(case.adapters["glass-lint"].forbidden()[0].line, Some(3));
}

#[test]
fn defaults_typescript_cases_from_the_fixture_extension() {
    let case = parse_case(
        Path::new("fixtures"),
        Path::new("fixtures/network/runtime.mts"),
        "// @tool glass-lint rules=js:network.request\nfetch('/remote');\n".into(),
    )
    .unwrap();

    assert_eq!(case.language, "typescript");
    assert_eq!(case.filename, "runtime.mts");
}

#[test]
fn rejects_a_language_that_conflicts_with_the_fixture_extension() {
    let root = crate::test_support::TempDir::new();
    std::fs::write(
        root.path().join("conflict.ts"),
        "// @case language javascript\n// @tool glass-lint rules=js:network.request\nfetch('/remote');\n",
    )
    .unwrap();

    let error = load_cases(root.path()).unwrap_err().to_string();
    assert!(error.contains("conflicts with its fixture extension"));
}

#[test]
fn rejects_legacy_competing_resolution_fields() {
    let error = toml::from_str::<ProjectResolutionManifest>(
        "importer = 'main.js'\nkind = 'import'\nrequest = 'pkg'\nline = 1\ncolumn = 1\nend_line = 1\nend_column = 4\npath = 'src/pkg.js'\npackage = 'pkg'\n",
    )
    .unwrap_err();
    assert!(error.to_string().contains("outcome"));
}
