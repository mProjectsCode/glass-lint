//! TypeScript parsing, runtime filtering, extension selection, and location
//! tests.
//!
//! The fixtures distinguish runtime syntax from type-only declarations and keep
//! source coordinates authoritative after TypeScript syntax is stripped.

use std::path::Path;

use glass_lint_core::{
    Environment, Linter, LinterConfig, RuleCatalog, SourceLanguage,
    project::SourceFile,
    rules::{Confidence, EventQuery, Rule, Severity},
};

/// Build the minimal TypeScript-capable linter used by every fixture.
fn linter() -> Linter {
    let rule = Rule::catalog_builder("network.fetch")
        .description("Uses fetch")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    Linter::new(LinterConfig::new(
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap()
}

fn typescript_source(path: &str, source: &str) -> SourceFile {
    SourceFile::with_language(path, source, SourceLanguage::TypeScript).unwrap()
}

#[test]
fn typed_runtime_calls_match_at_original_locations() {
    let report = linter()
        .lint_source(typescript_source(
            "input.ts",
            "const call = (url: string): void => fetch(url as string);",
        ))
        .unwrap();
    assert!(!report.files()[0].has_parse_diagnostics());
    assert_eq!(report.files()[0].findings().len(), 1);
    assert_eq!(
        report.files()[0].findings()[0]
            .location()
            .range()
            .start()
            .line(),
        1
    );
    assert_eq!(
        report.files()[0].findings()[0]
            .location()
            .range()
            .start()
            .column(),
        37
    );
}

#[test]
fn assertions_and_type_annotations_do_not_move_runtime_locations() {
    let report = linter()
        .lint_source(typescript_source(
            "input.ts",
            "const value: string = (fetch! as (url: string) => string)('/data');",
        ))
        .unwrap();
    assert!(!report.files()[0].has_parse_diagnostics());
    assert_eq!(report.files()[0].findings().len(), 1);
    assert_eq!(
        report.files()[0].findings()[0]
            .location()
            .range()
            .start()
            .line(),
        1
    );
    assert_eq!(
        report.files()[0].findings()[0]
            .location()
            .range()
            .start()
            .column(),
        24
    );
}

#[test]
fn type_only_api_lookalikes_do_not_create_findings() {
    let report = linter().lint_source(typescript_source(
        "input.ts",
        "interface Fetch { call(): void }\ntype Alias = typeof fetch;\nimport type { fetch as imported } from 'api';\ndeclare function fetch(url: string): void;",
    ))
    .unwrap();
    assert!(!report.files()[0].has_parse_diagnostics());
    assert!(report.files()[0].findings().is_empty());
}

#[test]
fn runtime_enum_calls_are_detected_without_matching_enum_lookalikes() {
    let report = linter().lint_source(typescript_source(
        "runtime.ts",
        "enum Local { fetch }\nenum Values { Remote = fetch('/remote') }\nnamespace window { export const fetch = 1 }",
    ))
    .unwrap();
    assert!(!report.files()[0].has_parse_diagnostics());
    assert_eq!(report.files()[0].findings().len(), 1);
    assert_eq!(
        report.files()[0].findings()[0]
            .location()
            .range()
            .start()
            .line(),
        2
    );
    assert_eq!(
        report.files()[0].findings()[0]
            .location()
            .range()
            .start()
            .column(),
        24
    );
}

#[test]
fn parameter_properties_and_namespace_names_do_not_create_global_provenance() {
    let report = linter().lint_source(typescript_source(
        "lookalikes.ts",
        "class Local { constructor(public fetch: unknown) {}\n  run() { this.fetch; }\n}\nnamespace fetch { export const value = 1 }",
    ))
    .unwrap();
    assert!(!report.files()[0].has_parse_diagnostics());
    assert!(report.files()[0].findings().is_empty());
}

#[test]
fn js_ts_unicode_crlf_locations_preserve_expected_ranges() {
    for (filename, source) in [
        (
            "input.js",
            "const cafe\u{301} = 'é';\r\nfetch('/data');\r\n",
        ),
        (
            "input.ts",
            "const cafe\u{301}: string = 'é';\r\nfetch('/data');\r\n",
        ),
    ] {
        let report = linter()
            .lint_source(
                if Path::new(filename)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
                {
                    typescript_source(filename, source)
                } else {
                    SourceFile::new(filename, source).unwrap()
                },
            )
            .unwrap();
        assert!(!report.files()[0].has_parse_diagnostics(), "{filename}");
        assert_eq!(report.files()[0].findings().len(), 1, "{filename}");
        let range = &report.files()[0].findings()[0].location().range();
        assert_eq!(
            (range.start().line(), range.start().column()),
            (2, 1),
            "{filename}"
        );
        assert_eq!(
            (range.end().line(), range.end().column()),
            (2, 6),
            "{filename}"
        );
    }
}

#[test]
fn virtual_sources_require_an_explicit_typescript_language() {
    assert_eq!(
        SourceFile::new("main.ts", "").unwrap().language(),
        SourceLanguage::JavaScript
    );
    assert_eq!(
        typescript_source("virtual", "").language(),
        SourceLanguage::TypeScript
    );
}

#[test]
fn module_specific_extensions_select_the_expected_parser() {
    for filename in ["input.cts", "input.mts"] {
        let report = linter()
            .lint_source(typescript_source(
                filename,
                "const value: string = fetch('/data');",
            ))
            .unwrap();
        assert!(!report.files()[0].has_parse_diagnostics(), "{filename}");
        assert_eq!(report.files()[0].findings().len(), 1, "{filename}");
    }
    for filename in ["input.cjs", "input.mjs"] {
        let report = linter()
            .lint_source(SourceFile::new(filename, "fetch('/data');").unwrap())
            .unwrap();
        assert!(!report.files()[0].has_parse_diagnostics(), "{filename}");
        assert_eq!(report.files()[0].findings().len(), 1, "{filename}");
    }
}

#[test]
fn malformed_typescript_reports_original_location() {
    let report = linter()
        .lint_source(typescript_source("broken.ts", "const value: = 1;"))
        .unwrap();
    assert_eq!(report.files()[0].parse_diagnostic_count(), 1);
    assert_eq!(
        report.files()[0].diagnostics()[0]
            .parse_diagnostic()
            .unwrap()
            .filename,
        "broken.ts"
    );
    assert_eq!(
        report.files()[0].diagnostics()[0]
            .parse_diagnostic()
            .unwrap()
            .range
            .as_ref()
            .unwrap()
            .start()
            .line(),
        1
    );
}
