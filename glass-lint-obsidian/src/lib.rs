//! Obsidian rule definitions and ready-to-use core linters.
//!
//! The provider owns Obsidian globals, rule profiles, and disclosure mapping;
//! matching and report primitives remain in the provider-neutral core crate.

use std::collections::BTreeSet;

use glass_lint_core::{AnalysisReport, Environment, LinterConfig, RuleCatalog, RuleMetadata};

mod catalog;
mod rules;

#[must_use]
/// Return metadata for every rule in the `obsidian:` provider catalog.
pub fn rule_metadata() -> Vec<RuleMetadata> {
    obsidian_catalog().metadata()
}

#[must_use]
pub fn obsidian_catalog() -> RuleCatalog {
    RuleCatalog::new("obsidian", catalog::obsidian_api_rules().to_vec())
        .expect("valid Obsidian catalog")
}

#[must_use]
pub fn obsidian_environment() -> Environment {
    let mut environment = glass_lint_js::electron_environment();
    environment
        .add_globals([
            "Notice",
            "activeDocument",
            "app",
            "moment",
            "request",
            "requestUrl",
        ])
        .expect("valid Obsidian globals");
    environment
        .add_global_object("activeWindow")
        .expect("valid Obsidian global object");
    environment
}

/// Return the complete core configuration for the Obsidian renderer target.
#[must_use]
pub fn obsidian_config() -> LinterConfig {
    LinterConfig::new(
        vec![
            glass_lint_js::js_catalog(),
            glass_lint_js::browser_catalog(),
            glass_lint_js::node_catalog(),
            glass_lint_js::electron_catalog(),
            obsidian_catalog(),
        ],
        obsidian_environment(),
    )
}

#[must_use]
/// Collect disclosure categories for findings in the `obsidian:` namespace.
pub fn disclosures_for_report(report: &AnalysisReport) -> BTreeSet<&'static str> {
    report
        .files
        .iter()
        .flat_map(|file| file.findings.iter())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_namespaced_and_unique() {
        let catalog = rule_metadata();
        assert!(!catalog.is_empty());
        assert!(
            catalog
                .iter()
                .all(|rule| rule.id.as_str().starts_with("obsidian:"))
        );
        let environment = obsidian_environment();
        assert!(environment.global_bindings().any(|name| name == "app"));
        assert!(!environment.global_bindings().any(|name| name == "Modal"));
        assert!(
            environment
                .global_bindings()
                .any(|name| name == "requestUrl")
        );
        assert!(
            environment
                .global_bindings()
                .any(|name| name == "activeDocument")
        );
        assert!(
            environment
                .global_objects()
                .any(|name| name == "activeWindow")
        );
    }

    #[test]
    fn active_window_is_a_configured_global_object() {
        use glass_lint_core::rules::{CallMatcher, Confidence, Rule, Severity};

        let rule = Rule::builder("test.eval")
            .description("eval")
            .category("test")
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .matcher(CallMatcher::global("eval"))
            .build()
            .unwrap();
        let report = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
            vec![RuleCatalog::new("test", vec![rule]).unwrap()],
            obsidian_environment(),
        ))
        .unwrap()
        .lint_snippet("activeWindow.eval('x')", "main.js")
        .unwrap();
        assert_eq!(report.files[0].findings.len(), 1);
    }

    #[test]
    fn active_window_shares_the_configured_environment() {
        use glass_lint_core::rules::{CallMatcher, Confidence, Rule, Severity};

        let rule = Rule::builder("test.request")
            .description("request")
            .category("test")
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .matcher(CallMatcher::global("requestUrl"))
            .build()
            .unwrap();
        let report = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
            vec![RuleCatalog::new("test", vec![rule]).unwrap()],
            obsidian_environment(),
        ))
        .unwrap()
        .lint_snippet(
            "requestUrl('/a'); window.requestUrl('/b'); activeWindow.requestUrl('/c');",
            "main.js",
        )
        .unwrap();
        assert_eq!(report.files[0].findings.len(), 3);
    }

    #[test]
    fn preconfigured_linter_reports_precise_network_calls() {
        let report = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
            vec![
                glass_lint_js::js_catalog(),
                glass_lint_js::browser_catalog(),
                glass_lint_js::node_catalog(),
                glass_lint_js::electron_catalog(),
                obsidian_catalog(),
            ],
            obsidian_environment(),
        ))
        .unwrap()
        .lint_snippet(
            "import { request } from 'obsidian';\nrequest('/one');\nrequest('/two');",
            "main.js",
        )
        .unwrap();
        let findings: Vec<_> = report.files[0]
            .findings
            .iter()
            .filter(|finding| finding.rule_id.as_str() == "obsidian:network.request")
            .collect();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].location.range.start().line(), 2);
        assert_eq!(findings[1].location.range.start().line(), 3);
    }

    #[test]
    fn disclosure_policy_is_applied_by_the_obsidian_adapter() {
        let report = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
            vec![
                glass_lint_js::js_catalog(),
                glass_lint_js::browser_catalog(),
                glass_lint_js::node_catalog(),
                glass_lint_js::electron_catalog(),
                obsidian_catalog(),
            ],
            obsidian_environment(),
        ))
        .unwrap()
        .lint_snippet(
            "import { request } from 'obsidian'; request('/network');",
            "main.js",
        )
        .unwrap();
        assert_eq!(
            disclosures_for_report(&report),
            BTreeSet::from([
                "disclosure.network_access",
                "disclosure.cors_free_network_access"
            ])
        );
    }
}
