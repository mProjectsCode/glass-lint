//! Generic JavaScript, browser, Node.js, and Electron rules.

use glass_lint_core::{LintReport, Linter, RuleCatalog, RuleId, RuleMetadata};
use std::collections::BTreeSet;

mod disclosures;
mod rules;

pub fn rule_catalog() -> Vec<RuleMetadata> {
    catalog().metadata()
}

pub fn recommended_linter() -> Linter {
    let catalog = catalog();
    let enabled = rules::all()
        .into_iter()
        .filter(|rule| rule.confidence() == glass_lint_core::rules::Confidence::High)
        .map(|rule| RuleId::parse(format!("js:{}", rule.id())).unwrap());
    Linter::with_rules(catalog, enabled).unwrap()
}

pub fn heuristic_linter() -> Linter {
    Linter::new(catalog())
}

pub fn disclosures_for_report(report: &LintReport) -> BTreeSet<&'static str> {
    report
        .findings
        .iter()
        .flat_map(|finding| {
            finding
                .rule_id
                .as_str()
                .strip_prefix("js:")
                .into_iter()
                .flat_map(|id| disclosures::for_rule(id).iter().copied())
        })
        .collect()
}

fn catalog() -> RuleCatalog {
    RuleCatalog::new("js", rules::all()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_is_namespaced() {
        assert!(
            rule_catalog()
                .iter()
                .all(|rule| rule.id.as_str().starts_with("js:"))
        );
    }
}
