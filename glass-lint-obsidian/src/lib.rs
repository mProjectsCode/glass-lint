//! Obsidian rule definitions and ready-to-use core linters.

use std::collections::BTreeSet;

use glass_lint_core::{LintReport, Linter, RuleCatalog, RuleId, RuleMetadata};

mod catalog;
mod rules;

pub fn rule_catalog() -> Vec<RuleMetadata> {
    catalog().metadata()
}

pub fn recommended_linter() -> Linter {
    let catalog = catalog();
    let enabled = catalog::obsidian_api_rules()
        .iter()
        .filter(|rule| rule.confidence == glass_lint_core::rules::Confidence::High)
        .map(|rule| RuleId::parse(format!("obsidian:{}", rule.id)).unwrap());
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
                .strip_prefix("obsidian:")
                .into_iter()
                .flat_map(|id| catalog::disclosures_for_rule(id).iter().copied())
        })
        .collect()
}

fn catalog() -> RuleCatalog {
    RuleCatalog::new("obsidian", catalog::obsidian_api_rules().to_vec()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_namespaced_and_unique() {
        let catalog = rule_catalog();
        assert!(!catalog.is_empty());
        assert!(
            catalog
                .iter()
                .all(|rule| rule.id.as_str().starts_with("obsidian:"))
        );
    }

    #[test]
    fn preconfigured_linter_reports_precise_network_calls() {
        let report = heuristic_linter().lint(
            "import { request } from 'obsidian';\nrequest('/one');\nrequest('/two');",
            "main.js",
        );
        let findings: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.rule_id.as_str() == "obsidian:network.request")
            .collect();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].range.start.line, 2);
        assert_eq!(findings[1].range.start.line, 3);
    }

    #[test]
    fn disclosure_policy_is_applied_by_the_obsidian_adapter() {
        let report = heuristic_linter().lint(
            "import { request } from 'obsidian'; request('/network');",
            "main.js",
        );
        assert_eq!(
            disclosures_for_report(&report),
            BTreeSet::from([
                "disclosure.network_access",
                "disclosure.cors_free_network_access"
            ])
        );
    }
}
