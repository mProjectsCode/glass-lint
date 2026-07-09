//! Obsidian rule definitions and ready-to-use core linters.

use std::collections::BTreeSet;

use glass_lint_core::{LintReport, Linter, RuleCatalog, RuleId, RuleMetadata};

mod catalog;

/// Metadata for every rule supplied by this provider.
pub fn rule_catalog() -> Vec<RuleMetadata> {
    catalog().metadata()
}

/// A precision-first linter suitable for normal use.
pub fn recommended_linter() -> Linter {
    let catalog = catalog();
    let enabled = catalog::obsidian_api_rules()
        .iter()
        .filter(|rule| rule.confidence == glass_lint_core::rules::Confidence::High)
        .map(|rule| {
            RuleId::parse(format!("obsidian:{}", rule.id))
                .expect("built-in Obsidian rule IDs are valid")
        });
    Linter::with_rules(catalog, enabled)
        .expect("the recommended profile only contains catalog rules")
}

/// A broad linter that enables every rule, including heuristic matches.
pub fn heuristic_linter() -> Linter {
    Linter::new(catalog())
}

/// Applies Obsidian disclosure policy to core findings.
///
/// The generic engine deliberately returns only capabilities; this adapter
/// keeps provider policy attached to the provider that owns it.
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
                .flat_map(|rule_id| catalog::disclosures_for_rule(rule_id).iter().copied())
        })
        .collect()
}

fn catalog() -> RuleCatalog {
    RuleCatalog::new("obsidian", catalog::obsidian_api_rules().to_vec())
        .expect("the built-in Obsidian catalog is valid")
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
        let report = heuristic_linter().lint("fetch('/one');\nfetch('/two');", "main.js");
        let findings: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.rule_id.as_str() == "obsidian:network.browser")
            .collect();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].range.start.line, 1);
        assert_eq!(findings[1].range.start.line, 2);
    }

    #[test]
    fn disclosure_policy_is_applied_by_the_obsidian_adapter() {
        let report = heuristic_linter().lint("fetch('/network');", "main.js");
        assert_eq!(
            disclosures_for_report(&report),
            BTreeSet::from(["disclosure.network_access"])
        );
    }
}
