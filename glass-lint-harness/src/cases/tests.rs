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
    let root = tempfile::tempdir().unwrap();
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

#[test]
fn parses_and_normalizes_bundle_profiles() {
    let source = "\
// @bundle obsidian,web
// @tool glass-lint rules=js:network.request
fetch('/remote');
";
    let case = parse_case(
        Path::new("fixtures"),
        Path::new("fixtures/network/bundled.js"),
        source.into(),
    )
    .unwrap();
    assert_eq!(
        case.bundles(),
        &[
            crate::types::BundleProfile::Web,
            crate::types::BundleProfile::Obsidian
        ]
    );
}

#[test]
fn rejects_invalid_bundle_directives() {
    for (directive, expected) in [
        ("", "at least one profile"),
        ("web,web", "duplicate bundle profile"),
        ("unknown", "unknown bundle profile"),
    ] {
        let source = format!(
            "// @bundle {directive}\n// @tool glass-lint rules=js:network.request\nfetch('/');\n"
        );
        let error = parse_case(
            Path::new("fixtures"),
            Path::new("fixtures/network/bundled.js"),
            source,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn rejects_late_and_duplicate_bundle_directives() {
    let late = "// @tool glass-lint rules=js:network.request\nfetch('/');\n// @bundle web\n";
    let error = parse_case(
        Path::new("fixtures"),
        Path::new("fixtures/network/late.js"),
        late.into(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("leading comment block"));

    let duplicate = "// @bundle web\n// @bundle obsidian\n// @tool glass-lint rules=js:network.request\nfetch('/');\n";
    let error = parse_case(
        Path::new("fixtures"),
        Path::new("fixtures/network/duplicate.js"),
        duplicate.into(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("only one @bundle"));

    let string_literal = "const text = \"// @bundle web\";\n// @tool glass-lint rules=js:network.request\nfetch('/');\n";
    assert!(
        parse_case(
            Path::new("fixtures"),
            Path::new("fixtures/network/string.js"),
            string_literal.into(),
        )
        .is_ok()
    );
}

#[test]
fn bundled_cases_require_the_canonical_tool() {
    let error = parse_case(
        Path::new("fixtures"),
        Path::new("fixtures/network/missing-tool.js"),
        "// @bundle web\nfetch('/');\n".into(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("must configure"));
}

#[test]
fn bundled_projects_require_one_declared_entry() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("case.toml"),
        "[tool.\"glass-lint\"]\nrules = [\"obsidian:network.request\"]\n",
    )
    .unwrap();
    std::fs::write(project.join("main.js"), "// @bundle web\nvar value = 1;\n").unwrap();
    std::fs::write(project.join("other.js"), "var other = 1;\n").unwrap();
    let error = load_cases(root.path()).unwrap_err().to_string();
    assert!(error.contains("explicitly declare exactly one entry"));
}

#[test]
fn bundled_projects_reject_multiple_entries_and_non_entry_metadata() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("case.toml"),
        "[case]\nentries = [\"main.js\", \"other.js\"]\n[tool.\"glass-lint\"]\nrules = [\"obsidian:network.request\"]\n",
    )
    .unwrap();
    std::fs::write(project.join("main.js"), "var value = 1;\n").unwrap();
    std::fs::write(project.join("other.js"), "// @bundle web\nvar other = 1;\n").unwrap();
    let error = load_cases(root.path()).unwrap_err().to_string();
    assert!(error.contains("exactly one entry"));
}
