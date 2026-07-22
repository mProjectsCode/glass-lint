//! Linter integration tests exercising public API paths.
//!
//! These tests verify that the linter's public interface produces correct
//! findings, does not produce false positives for shadowed lookalikes,
//! handles evidence bounding, and respects rule selection.

use glass_lint_core::{
    Environment, LintConfigError, Linter, LinterConfig, RuleBaseline, RuleCatalog, RuleId,
    RuleOverride, RuleSelection, RuleState,
    project::types::DiagnosticKind,
    rules::{Confidence, Matcher, Rule, Severity},
};

fn catalog() -> RuleCatalog {
    let rule = Rule::builder("network.fetch")
        .description("Uses fetch")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    RuleCatalog::new("test", vec![rule]).unwrap()
}

fn test_linter(catalog: RuleCatalog, environment: Environment) -> Linter {
    Linter::new(LinterConfig::new(vec![catalog], environment)).unwrap()
}

fn catalog_linter(catalog: RuleCatalog) -> Linter {
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    test_linter(catalog, environment)
}

fn snippet(linter: &Linter, source: &str, filename: &str) -> glass_lint_core::AnalysisReport {
    linter.lint_snippet(source, filename).unwrap()
}

#[test]
fn emits_one_located_finding_per_match() {
    let report = snippet(
        &catalog_linter(catalog()),
        "fetch('/a');\nfetch('/b');",
        "input.js",
    );
    assert_eq!(report.files[0].findings.len(), 2);
    assert_eq!(report.files[0].findings[0].location.range.start().line(), 1);
    assert_eq!(report.files[0].findings[1].location.range.start().line(), 2);
    assert_eq!(report.files[0].findings[0].evidence.len(), 1);
    assert_eq!(report.files[0].findings[1].evidence.len(), 1);
    assert_eq!(
        report.files[0].findings[0].evidence[0].message,
        "call of \"fetch\""
    );
    assert_eq!(
        report.files[0].findings[0].evidence[0]
            .location
            .as_ref()
            .map(|location| &location.range),
        Some(&report.files[0].findings[0].location.range)
    );
    assert_eq!(
        report.files[0].findings[1].evidence[0]
            .location
            .as_ref()
            .map(|location| &location.range),
        Some(&report.files[0].findings[1].location.range)
    );
}

#[test]
fn findings_only_carry_evidence_for_their_own_location() {
    let rule = Rule::builder("vault.write")
        .description("Writes vault files")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.vault.create"))
        .matcher(Matcher::rooted_member_call("app.vault.createFolder"))
        .build()
        .unwrap();
    let report = snippet(
        &test_linter(
            RuleCatalog::new("test", vec![rule]).unwrap(),
            Environment::default(),
        ),
        "this.app.vault.create('a');\nthis.app.vault.createFolder('b');",
        "input.js",
    );

    assert_eq!(report.files[0].findings.len(), 2);
    assert_eq!(report.files[0].findings[0].evidence.len(), 1);
    assert_eq!(
        report.files[0].findings[0].evidence[0].message,
        "member_call of \"app.vault.create\""
    );
    assert_eq!(report.files[0].findings[1].evidence.len(), 1);
    assert_eq!(
        report.files[0].findings[1].evidence[0].message,
        "member_call of \"app.vault.createFolder\""
    );
}

#[test]
fn rejects_shadowed_global_lookalikes() {
    let report = snippet(
        &catalog_linter(catalog()),
        "function demo(fetch) { fetch('/local'); } fetch('/global');",
        "input.js",
    );
    assert_eq!(report.files[0].findings.len(), 1);
}

#[test]
fn collapses_contained_ranges_for_same_rule() {
    let rule = Rule::builder("metadata.read")
        .description("Reads metadata")
        .category("metadata")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_read("app.metadataCache"))
        .matcher(Matcher::rooted_member_call(
            "app.metadataCache.getFileCache",
        ))
        .build()
        .unwrap();
    let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
    let report = snippet(
        &test_linter(catalog, Environment::default()),
        "this.app.metadataCache.getFileCache(file);",
        "input.js",
    );

    assert_eq!(report.files[0].findings.len(), 1);
    assert_eq!(
        report.files[0].findings[0].location.range.start().column(),
        1
    );
    assert_eq!(
        report.files[0].findings[0].location.range.end().column(),
        36
    );
    assert_eq!(report.files[0].findings[0].evidence.len(), 2);
    assert!(report.files[0].findings[0].evidence.iter().all(|evidence| {
        evidence.location.as_ref().is_some_and(|location| {
            report.files[0].findings[0]
                .location
                .range
                .contains(&location.range)
        })
    }));
}

#[test]
fn validates_custom_rule_selection() {
    let unknown = RuleId::parse("test:missing").unwrap();
    assert!(matches!(
        Linter::new(
            LinterConfig::new(vec![catalog()], Environment::default()).with_rules(
                RuleSelection::new(RuleBaseline::None).with_override(
                    RuleOverride::new(unknown.to_string(), RuleState::Enabled).unwrap(),
                ),
            ),
        ),
        Err(LintConfigError::UnknownRule(_))
    ));
}

#[test]
fn ordered_rule_overrides_select_stable_catalog_indexes() {
    let first = Rule::builder("network.first")
        .description("First")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    let second = Rule::builder("network.second")
        .description("Second")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    let catalog = RuleCatalog::new("test", vec![first, second]).unwrap();
    let selection = RuleSelection::new(RuleBaseline::None)
        .with_override(RuleOverride::new("test:*", RuleState::Enabled).unwrap())
        .with_override(RuleOverride::new("test:network.first", RuleState::Disabled).unwrap());
    let linter =
        Linter::new(LinterConfig::new(vec![catalog], Environment::default()).with_rules(selection))
            .unwrap();
    assert_eq!(
        linter.enabled_rule_ids(),
        vec![RuleId::parse("test:network.second").unwrap()]
    );
}

#[test]
fn selectors_require_a_known_match() {
    let catalog = RuleCatalog::new("test", vec![]).unwrap();
    let selection = RuleSelection::new(RuleBaseline::None)
        .with_override(RuleOverride::new("test:missing", RuleState::Enabled).unwrap());
    assert!(matches!(
        Linter::new(LinterConfig::new(vec![catalog], Environment::default()).with_rules(selection)),
        Err(LintConfigError::UnknownRule(_))
    ));
}

#[test]
fn reports_structured_diagnostic_for_oversized_source() {
    let report = snippet(
        &catalog_linter(catalog()),
        &"x".repeat(glass_lint_core::MAX_SOURCE_BYTES + 1),
        "large.js",
    );
    assert!(report.files[0].findings.is_empty());
    assert_eq!(report.files[0].parse_diagnostic_count(), 1);
    assert_eq!(
        report.files[0].diagnostics[0]
            .parse_diagnostic()
            .unwrap()
            .code,
        DiagnosticKind::SourceTooLarge.into()
    );
    assert_eq!(
        report.files[0].diagnostics[0]
            .parse_diagnostic()
            .unwrap()
            .filename,
        "large.js"
    );
    assert!(
        report.files[0].diagnostics[0]
            .parse_diagnostic()
            .unwrap()
            .range
            .is_none()
    );
}

#[test]
fn parse_diagnostics_carry_stable_location_context() {
    let report = snippet(&catalog_linter(catalog()), "fetch(", "broken.js");
    assert!(report.files[0].findings.is_empty());
    let diagnostic = &report.files[0].diagnostics[0].parse_diagnostic().unwrap();
    assert_eq!(diagnostic.code, DiagnosticKind::SyntaxError.into());
    assert_eq!(diagnostic.filename, "broken.js");
    assert!(diagnostic.message.starts_with("JavaScript parse error:"));
    assert!(diagnostic.range.is_some());
}

#[test]
fn source_locations_handle_crlf_and_eof_without_byte_columns() {
    let report = snippet(
        &catalog_linter(catalog()),
        "fetch('/a');\r\nfetch('/é');",
        "crlf.js",
    );
    assert_eq!(report.files[0].findings.len(), 2);
    assert_eq!(report.files[0].findings[0].location.range.start().line(), 1);
    assert_eq!(report.files[0].findings[1].location.range.start().line(), 2);
    assert!(
        report.files[0].findings[1].location.range.end().column()
            > report.files[0].findings[1].location.range.start().column()
    );

    let empty = snippet(&catalog_linter(catalog()), "", "empty.js");
    assert!(empty.files[0].findings.is_empty());
    assert!(!empty.files[0].has_parse_diagnostics());
}

#[test]
fn evidence_ranges_and_snippets_are_populated_for_unicode_source() {
    let report = snippet(
        &catalog_linter(catalog()),
        "// é\nfetch('/x');",
        "unicode.js",
    );
    let evidence = &report.files[0].findings[0].evidence[0];
    assert_eq!(
        evidence
            .location
            .as_ref()
            .map(|location| location.range.start().line()),
        Some(2)
    );
}

#[test]
fn evidence_limit_is_source_ordered_and_applied_once() {
    let source = (0..20).map(|_| "fetch();\n").collect::<String>();
    let report = snippet(&catalog_linter(catalog()), &source, "many.js");
    assert_eq!(report.files[0].findings.len(), 20);
    assert_eq!(
        report.files[0]
            .findings
            .first()
            .unwrap()
            .location
            .range
            .start()
            .line(),
        1
    );
    assert_eq!(
        report.files[0]
            .findings
            .last()
            .unwrap()
            .location
            .range
            .start()
            .line(),
        20
    );
}

#[test]
fn enabled_rule_order_does_not_affect_findings() {
    let rule_a = Rule::builder("alpha.first")
        .description("First")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    let rule_b = Rule::builder("beta.second")
        .description("Second")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("XMLHttpRequest"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment
        .add_globals(["fetch", "XMLHttpRequest"])
        .unwrap();
    let catalog = RuleCatalog::new("test", vec![rule_a, rule_b]).unwrap();

    let source = "fetch('/a'); new XMLHttpRequest();";
    let enabled = RuleSelection::new(RuleBaseline::None)
        .with_override(RuleOverride::new("test:alpha.first", RuleState::Enabled).unwrap())
        .with_override(RuleOverride::new("test:beta.second", RuleState::Enabled).unwrap());
    let report_asc = snippet(
        &Linter::new(
            LinterConfig::new(vec![catalog.clone()], environment.clone()).with_rules(enabled),
        )
        .unwrap(),
        source,
        "order.js",
    );
    let enabled = RuleSelection::new(RuleBaseline::None)
        .with_override(RuleOverride::new("test:beta.second", RuleState::Enabled).unwrap())
        .with_override(RuleOverride::new("test:alpha.first", RuleState::Enabled).unwrap());
    let report_desc = snippet(
        &Linter::new(LinterConfig::new(vec![catalog], environment).with_rules(enabled)).unwrap(),
        source,
        "order.js",
    );

    assert_eq!(
        report_asc.files[0].findings.len(),
        report_desc.files[0].findings.len()
    );
    for (a, b) in report_asc.files[0]
        .findings
        .iter()
        .zip(report_desc.files[0].findings.iter())
    {
        assert_eq!(a.rule_id, b.rule_id);
        assert_eq!(a.location.range, b.location.range);
        assert_eq!(a.message, b.message);
    }
}

#[test]
fn disabled_catalog_rules_do_not_produce_findings() {
    let rule_a = Rule::builder("alpha.first")
        .description("First")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    let rule_b = Rule::builder("beta.second")
        .description("Second")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("XMLHttpRequest"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment
        .add_globals(["fetch", "XMLHttpRequest"])
        .unwrap();
    let catalog = RuleCatalog::new("test", vec![rule_a, rule_b]).unwrap();
    let selection = RuleSelection::new(RuleBaseline::None)
        .with_override(RuleOverride::new("test:beta.second", RuleState::Enabled).unwrap());
    let report = snippet(
        &Linter::new(LinterConfig::new(vec![catalog], environment).with_rules(selection)).unwrap(),
        "fetch(); XMLHttpRequest();",
        "subset.js",
    );
    assert_eq!(report.files[0].findings.len(), 1);
    assert_eq!(
        report.files[0].findings[0].rule_id.as_str(),
        "test:beta.second"
    );
}

#[test]
fn combines_provider_rules_with_overlapping_local_ids() {
    let first = Rule::builder("network.request")
        .description("First provider request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    let second = Rule::builder("network.request")
        .description("Second provider request")
        .category("network")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("requestUrl"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment.add_globals(["fetch", "requestUrl"]).unwrap();
    let linter = Linter::new(LinterConfig::new(
        vec![
            RuleCatalog::new("first", vec![first]).unwrap(),
            RuleCatalog::new("second", vec![second]).unwrap(),
        ],
        environment,
    ))
    .unwrap();

    let report = snippet(&linter, "fetch('/a'); requestUrl('/b');", "combined.js");
    assert_eq!(report.files[0].findings.len(), 2);
    assert_eq!(
        report.files[0].findings[0].rule_id.as_str(),
        "first:network.request"
    );
    assert_eq!(
        report.files[0].findings[1].rule_id.as_str(),
        "second:network.request"
    );
}

#[test]
fn combined_linter_preserves_each_input_rule_selection() {
    let enabled_rule = Rule::builder("enabled")
        .description("Enabled")
        .category("test")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("fetch"))
        .build()
        .unwrap();
    let disabled_rule = Rule::builder("disabled")
        .description("Disabled")
        .category("test")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::global_call("requestUrl"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment.add_globals(["fetch", "requestUrl"]).unwrap();
    let selection = RuleSelection::new(RuleBaseline::None)
        .with_override(RuleOverride::new("first:enabled", RuleState::Enabled).unwrap());
    let report = snippet(
        &Linter::new(
            LinterConfig::new(
                vec![
                    RuleCatalog::new("first", vec![enabled_rule]).unwrap(),
                    RuleCatalog::new("second", vec![disabled_rule]).unwrap(),
                ],
                environment,
            )
            .with_rules(selection),
        )
        .unwrap(),
        "fetch(); requestUrl();",
        "selection.js",
    );

    assert_eq!(report.files[0].findings.len(), 1);
    assert_eq!(
        report.files[0].findings[0].rule_id.as_str(),
        "first:enabled"
    );
}
