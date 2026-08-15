use super::*;

fn build(id: &str) -> Result<Rule, RuleBuildError> {
    Rule::catalog_builder(id)
        .description("rule")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::call_global("fetch"))
        .build()
}

#[test]
fn rejects_noncanonical_rule_ids() {
    for id in [
        "Network.fetch",
        ".network",
        "network.",
        "network..fetch",
        "network:fetch",
    ] {
        assert!(matches!(build(id), Err(RuleBuildError::InvalidId(_))));
    }
}

#[test]
fn reports_displayable_rule_id_errors() {
    let error = build("UPPER").unwrap_err();
    assert!(error.to_string().contains("invalid rule ID"));
}

#[test]
fn rejects_duplicate_required_metadata() {
    let cases = [
        (
            "description",
            Rule::catalog_builder("network.fetch")
                .description("one")
                .description("two"),
        ),
        (
            "severity",
            Rule::catalog_builder("network.fetch")
                .severity(Severity::Info)
                .severity(Severity::Warning),
        ),
        (
            "confidence",
            Rule::catalog_builder("network.fetch")
                .confidence(Confidence::High)
                .confidence(Confidence::Medium),
        ),
    ];
    for (field, builder) in cases {
        assert!(matches!(
            builder.build(),
            Err(RuleBuildError::DuplicateField(actual)) if actual == field
        ));
    }
}

#[test]
fn reports_first_duplicate_required_metadata() {
    let error = Rule::builder("network.fetch")
        .description("one")
        .description("two")
        .build()
        .expect_err("duplicate metadata should fail");

    assert_eq!(error, RuleBuildError::DuplicateField("description"));
}

#[test]
fn rejects_empty_and_incomplete_matchers() {
    assert!(
        Rule::catalog_builder("test.test")
            .description("desc")
            .severity(Severity::Warning)
            .confidence(Confidence::Medium)
            .build()
            .is_err_and(|error| error == RuleBuildError::MissingQuery)
    );
}

#[test]
fn registers_query_iterators_in_declaration_order() {
    let rule = Rule::catalog_builder("network.fetch")
        .description("rule")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .queries([
            EventQuery::call_global("fetch"),
            EventQuery::call_global("request"),
        ])
        .build()
        .unwrap();

    assert_eq!(rule.queries().len(), 2);
}

#[test]
fn try_query_reports_constructor_errors_at_the_call_site() {
    let error = Rule::builder("network.fetch")
        .try_query(EventQuery::call_global(""))
        .unwrap_err();
    assert!(matches!(error, QueryBuildError::EmptyIdentityName));
}
