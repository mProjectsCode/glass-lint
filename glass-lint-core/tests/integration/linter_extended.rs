use super::*;

#[test]
fn disabled_catalog_rules_do_not_produce_findings() {
    let rule_a = Rule::catalog_builder("alpha.first")
        .description("First")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let rule_b = Rule::catalog_builder("beta.second")
        .description("Second")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("XMLHttpRequest"))
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
    assert_eq!(report.files()[0].findings().len(), 1);
    assert_eq!(
        report.files()[0].findings()[0].rule_id().as_str(),
        "test:beta.second"
    );
}

#[test]
fn combines_provider_rules_with_overlapping_local_ids() {
    let first = Rule::catalog_builder("network.request")
        .description("First provider request")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let second = Rule::catalog_builder("network.request")
        .description("Second provider request")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("requestUrl"))
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
    assert_eq!(report.files()[0].findings().len(), 2);
    assert_eq!(
        report.files()[0].findings()[0].rule_id().as_str(),
        "first:network.request"
    );
    assert_eq!(
        report.files()[0].findings()[1].rule_id().as_str(),
        "second:network.request"
    );
}

#[test]
fn combined_linter_preserves_each_input_rule_selection() {
    let enabled_rule = Rule::catalog_builder("enabled")
        .description("Enabled")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
        .unwrap();
    let disabled_rule = Rule::catalog_builder("disabled")
        .description("Disabled")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("requestUrl"))
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

    assert_eq!(report.files()[0].findings().len(), 1);
    assert_eq!(
        report.files()[0].findings()[0].rule_id().as_str(),
        "first:enabled"
    );
}
