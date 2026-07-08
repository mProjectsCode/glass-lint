//! Obsidian rule definitions and ready-to-use core linters.

use glass_lint_core::{Linter, RuleCatalog, RuleId, RuleMetadata};

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
}
