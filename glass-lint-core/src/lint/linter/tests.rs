use crate::{
    Environment, LintConfigError, Linter, LinterConfig, RuleBaseline, RuleCatalog, RuleOverride,
    RuleSelection, RuleState,
    rules::{Confidence, EventQuery, Rule, Severity},
};

#[test]
fn findings_are_sorted_by_position() {
    let rule = Rule::catalog_builder("network.request")
        .description("Uses fetch")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    let linter = Linter::new(LinterConfig::new(
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap();

    let report = linter
        .lint_source(
            crate::project::SourceFile::new("sort.js", "fetch('/b'); fetch('/a');").unwrap(),
        )
        .unwrap();
    // Findings should be sorted by line, then column, then rule ID.
    assert_eq!(report.files()[0].findings().len(), 2);
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
        1
    );
    assert_eq!(
        report.files()[0].findings()[1]
            .location()
            .range()
            .start()
            .column(),
        14
    );
}

#[test]
fn classify_groups_findings_by_rule() {
    let rule = Rule::catalog_builder("network.request")
        .description("Uses fetch")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let mut environment = Environment::default();
    environment.add_global("fetch").unwrap();
    let linter = Linter::new(LinterConfig::new(
        vec![RuleCatalog::new("test", vec![rule]).unwrap()],
        environment,
    ))
    .unwrap();

    let report = linter
        .lint_source(
            crate::project::SourceFile::new("classify.js", "fetch('/a'); fetch('/b');").unwrap(),
        )
        .unwrap();
    assert_eq!(report.files()[0].findings().len(), 2);
    assert_eq!(
        report.files()[0].findings()[0].rule_id().as_str(),
        "test:network.request"
    );
}

#[test]
fn missing_selected_rule_fails_closed() {
    let selection = RuleSelection::new(RuleBaseline::None)
        .with_override(RuleOverride::new("unknown:missing", RuleState::Enabled).unwrap());
    let result = Linter::new(
        LinterConfig::new(
            vec![RuleCatalog::new("test", vec![]).unwrap()],
            Environment::default(),
        )
        .with_rules(selection),
    );
    assert!(matches!(result, Err(LintConfigError::UnknownRule(_))));
}
