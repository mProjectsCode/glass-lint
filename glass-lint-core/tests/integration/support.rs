//! Shared test utilities for glass-lint-core integration tests.
use std::collections::BTreeSet;

use glass_lint_core::{
    Environment, Linter, LinterConfig, MatchCertainty, RuleCatalog,
    project::{AnalysisReport, SourceFile},
    rules::{Builder, Confidence, Rule, Severity},
};

/// The stable facts most matcher tests need from an analysis report.
pub struct Classification {
    pub finding_count: usize,
    pub rule_ids: BTreeSet<String>,
    pub certainties: Vec<MatchCertainty>,
}

impl Classification {
    pub fn has_capability(&self, id: &str) -> bool {
        self.rule_ids.contains(&format!("test:{id}"))
    }
}

/// Build a standard test rule with the given id.
pub fn rule(id: &str) -> Builder {
    Rule::builder(id)
        .description(id)
        .severity(Severity::Info)
        .confidence(Confidence::High)
}

/// Create a linter from rules and an environment.
pub fn linter_from_catalog(catalog: RuleCatalog, environment: Environment) -> Linter {
    Linter::new(LinterConfig::new(vec![catalog], environment)).unwrap()
}

/// Lint one source with a caller-supplied catalog and environment.
pub fn lint_report_with_rules(
    source: &str,
    filename: &str,
    rules: &[Rule],
    environment: Environment,
) -> AnalysisReport {
    let catalog = RuleCatalog::new("test", rules.to_vec()).unwrap();
    linter_from_catalog(catalog, environment)
        .lint_source(SourceFile::new(filename, source).unwrap())
        .unwrap()
}

/// Lint one source with a caller-supplied environment.
pub fn lint_report_with_environment(
    source: &str,
    filename: &str,
    rule: Rule,
    environment: Environment,
) -> AnalysisReport {
    lint_report_with_rules(source, filename, &[rule], environment)
}

/// Lint one JavaScript snippet with the standard test environment.
pub fn lint_report(source: &str, rule: Rule) -> AnalysisReport {
    lint_report_with_environment(source, "test.js", rule, test_environment())
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

/// Lint with a standard rule and a list of additional global names.
pub fn lint_report_with_globals<I, S>(
    source: &str,
    filename: &str,
    rule: Rule,
    globals: I,
) -> AnalysisReport
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut environment = Environment::default();
    environment
        .add_globals(globals.into_iter().map(|global| global.as_ref().to_owned()))
        .unwrap();
    lint_report_with_environment(source, filename, rule, environment)
}

/// Lint multiple rules and retain only deterministic matching facts.
pub fn classify(source: &str, rules: &[Rule]) -> Classification {
    classify_with_environment(source, rules, test_environment())
}

pub fn classify_with_environment(
    source: &str,
    rules: &[Rule],
    environment: Environment,
) -> Classification {
    let report = lint_report_with_rules(source, "matcher.js", rules, environment);
    let count = report.files()[0].findings().len();
    let certainties = report.files()[0]
        .findings()
        .iter()
        .map(glass_lint_core::project::Finding::certainty)
        .collect();
    let ids = report.files()[0]
        .findings()
        .iter()
        .map(|finding| finding.rule_id().as_str().to_owned())
        .collect();
    Classification {
        finding_count: count,
        rule_ids: ids,
        certainties,
    }
}
