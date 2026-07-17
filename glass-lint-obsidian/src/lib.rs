//! Obsidian rule definitions and ready-to-use core linters.
//!
//! The provider owns Obsidian globals, rule profiles, and disclosure mapping;
//! matching and report primitives remain in the provider-neutral core crate.

use std::collections::BTreeSet;

use glass_lint_core::{AnalysisReport, Environment, Linter, RuleCatalog, RuleMetadata};

mod catalog;
mod rules;

#[must_use]
/// Return metadata for every rule in the `obsidian:` provider catalog.
pub fn rule_catalog() -> Vec<RuleMetadata> {
    catalog(default_environment()).metadata()
}

#[must_use]
/// Build the recommended high-confidence Obsidian linter.
pub fn recommended_linter() -> Linter {
    recommended_linter_with_environment(default_environment())
}

/// Build the recommended linter with an exact caller-supplied environment.
#[must_use]
/// Build the recommended linter with a caller-supplied environment.
pub fn recommended_linter_with_environment(environment: Environment) -> Linter {
    let catalog = catalog(environment);
    Linter::with_confidence(catalog, glass_lint_core::rules::Confidence::High)
}

#[must_use]
/// Build the complete Obsidian linter, including heuristic rules.
pub fn heuristic_linter() -> Linter {
    heuristic_linter_with_environment(default_environment())
}

/// Build the complete linter with an exact caller-supplied environment.
#[must_use]
/// Build the complete linter with a caller-supplied environment.
pub fn heuristic_linter_with_environment(environment: Environment) -> Linter {
    Linter::new(catalog(environment))
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

/// Globals provided by the Obsidian Electron renderer.
///
/// `activeWindow` is treated as sharing the same environment as the current
/// window. The runtime may return either the main window or a pop-out window,
/// and static analysis cannot determine which one is in use at a call site.
#[must_use]
pub fn default_environment() -> Environment {
    let mut environment = Environment::default();
    environment
        .add_globals([
            "Buffer",
            "EventSource",
            "Notification",
            "Notice",
            "URL",
            "URLSearchParams",
            "WebSocket",
            "XMLHttpRequest",
            "activeDocument",
            "app",
            "caches",
            "clearImmediate",
            "clearInterval",
            "clearTimeout",
            "console",
            "document",
            "fetch",
            "indexedDB",
            "localStorage",
            "module",
            "moment",
            "navigator",
            "process",
            "queueMicrotask",
            "require",
            "request",
            "requestUrl",
            "sessionStorage",
            "setImmediate",
            "setInterval",
            "setTimeout",
        ])
        .expect("built-in Obsidian environment names are valid");
    for name in ["window", "self", "global"] {
        environment
            .add_global_object(name)
            .expect("built-in Obsidian global-object names are valid");
    }
    environment
        .add_global_object("activeWindow")
        .expect("activeWindow global-object name is valid");
    environment
}

fn catalog(environment: Environment) -> RuleCatalog {
    // Apply the environment only at catalog construction so all profiles use
    // the same provider rule set and differ only by confidence filtering.
    RuleCatalog::with_environment(
        "obsidian",
        catalog::obsidian_api_rules().to_vec(),
        environment,
    )
    .unwrap()
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
        let environment = default_environment();
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
            .label("eval")
            .category("test")
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .matcher(CallMatcher::global("eval"))
            .build()
            .unwrap();
        let catalog =
            RuleCatalog::with_environment("test", vec![rule], default_environment()).unwrap();
        let report = Linter::new(catalog)
            .lint_snippet("activeWindow.eval('x')", "main.js")
            .unwrap();
        assert_eq!(report.files[0].findings.len(), 1);
    }

    #[test]
    fn active_window_shares_the_configured_environment() {
        use glass_lint_core::rules::{CallMatcher, Confidence, Rule, Severity};

        let rule = Rule::builder("test.request")
            .label("request")
            .category("test")
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .matcher(CallMatcher::global("requestUrl"))
            .build()
            .unwrap();
        let catalog =
            RuleCatalog::with_environment("test", vec![rule], default_environment()).unwrap();
        let report = Linter::new(catalog)
            .lint_snippet(
                "requestUrl('/a'); window.requestUrl('/b'); activeWindow.requestUrl('/c');",
                "main.js",
            )
            .unwrap();
        assert_eq!(report.files[0].findings.len(), 3);
    }

    #[test]
    fn preconfigured_linter_reports_precise_network_calls() {
        let report = heuristic_linter()
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
        let report = heuristic_linter()
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
