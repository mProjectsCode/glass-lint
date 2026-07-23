//! Shared test utilities for glass-lint-core integration tests.
//!
//! Import with `#[path = "support/mod.rs"] mod support;` in test files.
//!
//! Each test crate includes this module independently, so not every function is
//! used in every compilation.

#![allow(dead_code)]

use glass_lint_core::{
    Environment, Linter, LinterConfig, RuleCatalog,
    project::FileReport,
    rules::{Builder, Confidence, Rule, Severity},
};

/// Build a standard test rule with the given id.
pub fn rule(id: &str) -> Builder {
    Rule::builder(id)
        .description(id)
        .category("test")
        .severity(Severity::Info)
        .confidence(Confidence::High)
}

/// Create a linter from rules and an environment.
pub fn test_linter(rules: Vec<Rule>, environment: Environment) -> Linter {
    let catalog = RuleCatalog::new("test", rules).unwrap();
    Linter::new(LinterConfig::new(vec![catalog], environment)).unwrap()
}

/// Create a linter from a single rule with the test environment.
pub fn test_linter_for(rule: Rule) -> Linter {
    test_linter(vec![rule], test_environment())
}

/// Lint a snippet with a single rule and return the file report.
pub fn lint_snippet(source: &str, rule: Rule) -> FileReport {
    test_linter_for(rule)
        .lint_snippet(source, "test.js")
        .unwrap()
        .files
        .into_iter()
        .next()
        .unwrap()
}

/// Return finding messages from a file report.
pub fn finding_messages(report: &FileReport) -> Vec<&str> {
    report
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect()
}

/// Default test environment with common globals.
pub fn test_environment() -> Environment {
    let mut environment = Environment::default();
    environment
        .add_globals([
            "app",
            "client",
            "document",
            "fetch",
            "host",
            "navigator",
            "require",
            "vault",
        ])
        .unwrap();
    for object in ["window", "self", "global"] {
        environment.add_global_object(object).unwrap();
    }
    environment
}

/// Lint a source snippet with a single rule and assert the exact finding count.
pub fn assert_count(source: &str, rule: Rule, expected: usize) {
    assert_count_with_env(source, rule, expected, &test_environment());
}

/// Lint a source snippet with a single rule and a caller-supplied environment,
/// asserting the exact finding count.
pub fn assert_count_with_env(source: &str, rule: Rule, expected: usize, environment: &Environment) {
    let catalog = RuleCatalog::new("test", vec![rule]).unwrap();
    let count = Linter::new(LinterConfig::new(vec![catalog], environment.clone()))
        .unwrap()
        .lint_snippet(source, "test.js")
        .unwrap()
        .files[0]
        .findings
        .len();
    assert_eq!(count, expected, "{source}");
}

/// Lint a source snippet with multiple rules and return finding count and rule
/// ids.
pub fn classify(source: &str, rules: &[Rule]) -> (usize, Vec<String>) {
    let environment = test_environment();
    let catalog = RuleCatalog::new("test", rules.to_vec()).unwrap();
    let report = Linter::new(LinterConfig::new(vec![catalog], environment))
        .unwrap()
        .lint_snippet(source, "test.js")
        .unwrap();
    let count = report.files[0].findings.len();
    let ids = report.files[0]
        .findings
        .iter()
        .map(|finding| finding.rule_id.as_str().to_owned())
        .collect();
    (count, ids)
}
