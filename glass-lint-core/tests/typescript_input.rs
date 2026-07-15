use glass_lint_core::{
    Environment, Linter, RuleCatalog, SourceLanguage,
    rules::{Confidence, Matcher, Rule, Severity},
};

fn linter() -> Linter {
    let rule = Rule::builder("network.fetch")
        .label("Uses fetch")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    Linter::new(RuleCatalog::with_environment("test", vec![rule], environment).unwrap())
}

#[test]
fn typed_runtime_calls_match_at_original_locations() {
    let report = linter().lint(
        "const call = (url: string): void => fetch(url as string);",
        "input.ts",
    );
    assert!(report.parse_diagnostics.is_empty());
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].range.start.line, 1);
    assert_eq!(report.findings[0].range.start.column, 37);
}

#[test]
fn assertions_and_type_annotations_do_not_move_runtime_locations() {
    let report = linter().lint(
        "const value: string = (fetch! as (url: string) => string)('/data');",
        "input.ts",
    );
    assert!(report.parse_diagnostics.is_empty());
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].range.start.line, 1);
    assert_eq!(report.findings[0].range.start.column, 24);
}

#[test]
fn type_only_api_lookalikes_do_not_create_findings() {
    let report = linter().lint(
        "interface Fetch { call(): void }\ntype Alias = typeof fetch;\nimport type { fetch as imported } from 'api';\ndeclare function fetch(url: string): void;",
        "input.ts",
    );
    assert!(report.parse_diagnostics.is_empty());
    assert!(report.findings.is_empty());
}

#[test]
fn runtime_enum_calls_are_detected_without_matching_enum_lookalikes() {
    let report = linter().lint(
        "enum Local { fetch }\nenum Values { Remote = fetch('/remote') }\nnamespace window { export const fetch = 1 }",
        "runtime.ts",
    );
    assert!(report.parse_diagnostics.is_empty());
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].range.start.line, 2);
    assert_eq!(report.findings[0].range.start.column, 24);
}

#[test]
fn parameter_properties_and_namespace_names_do_not_create_global_provenance() {
    let report = linter().lint(
        "class Local { constructor(public fetch: unknown) {}\n  run() { this.fetch; }\n}\nnamespace fetch { export const value = 1 }",
        "lookalikes.ts",
    );
    assert!(report.parse_diagnostics.is_empty());
    assert!(report.findings.is_empty());
}

#[test]
fn unicode_and_crlf_preserve_typescript_finding_location() {
    let report = linter().lint(
        "const café: string = 'ok';\r\nfetch('/data');\r\n",
        "input.ts",
    );
    assert!(report.parse_diagnostics.is_empty());
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].range.start.line, 2);
    assert_eq!(report.findings[0].range.start.column, 1);
}

#[test]
fn language_is_selected_by_filename() {
    for filename in ["main.ts", "main.cts", "main.mts"] {
        assert_eq!(
            SourceLanguage::from_filename(filename),
            SourceLanguage::TypeScript
        );
        assert!(SourceLanguage::is_supported_filename(filename));
    }
    for filename in ["main.js", "main.cjs", "main.mjs"] {
        assert_eq!(
            SourceLanguage::from_filename(filename),
            SourceLanguage::JavaScript
        );
        assert!(SourceLanguage::is_supported_filename(filename));
    }
    for filename in ["main.d.ts", "main.d.cts", "main.d.mts"] {
        assert!(!SourceLanguage::is_supported_filename(filename));
    }
    assert_eq!(
        SourceLanguage::from_filename("MAIN.MTS"),
        SourceLanguage::TypeScript
    );
    assert_eq!(
        SourceLanguage::from_filename("virtual"),
        SourceLanguage::JavaScript
    );
}

#[test]
fn module_specific_extensions_select_the_expected_parser() {
    for filename in ["input.cts", "input.mts"] {
        let report = linter().lint("const value: string = fetch('/data');", filename);
        assert!(report.parse_diagnostics.is_empty(), "{filename}");
        assert_eq!(report.findings.len(), 1, "{filename}");
    }
    for filename in ["input.cjs", "input.mjs"] {
        let report = linter().lint("fetch('/data');", filename);
        assert!(report.parse_diagnostics.is_empty(), "{filename}");
        assert_eq!(report.findings.len(), 1, "{filename}");
    }
}

#[test]
fn malformed_typescript_reports_original_location() {
    let report = linter().lint("const value: = 1;", "broken.ts");
    assert_eq!(report.parse_diagnostics.len(), 1);
    assert_eq!(report.parse_diagnostics[0].filename, "broken.ts");
    assert_eq!(
        report.parse_diagnostics[0]
            .range
            .as_ref()
            .unwrap()
            .start
            .line,
        1
    );
}
