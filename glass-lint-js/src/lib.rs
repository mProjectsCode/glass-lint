//! Generic JavaScript, browser, Node.js, and Electron rules.
//!
//! This crate owns the provider namespace, its default host environment, and
//! the recommended/heuristic catalog profiles while delegating matching to
//! core.

use std::collections::BTreeSet;

use glass_lint_core::{AnalysisReport, Environment, Linter, RuleCatalog, RuleMetadata};

mod disclosures;
mod rules;

#[must_use]
/// Return metadata for every rule in the `js:` provider catalog.
pub fn rule_catalog() -> Vec<RuleMetadata> {
    catalog(default_environment()).metadata()
}

#[must_use]
/// Build the recommended high-confidence JavaScript linter.
pub fn recommended_linter() -> Linter {
    recommended_linter_with_environment(default_environment())
}

/// Build the recommended linter with an exact caller-supplied environment.
#[must_use]
/// Build the recommended linter with the provider's default environment.
pub fn recommended_linter_with_environment(environment: Environment) -> Linter {
    let catalog = catalog(environment);
    Linter::with_confidence(catalog, glass_lint_core::rules::Confidence::High)
}

#[must_use]
/// Build the complete JavaScript linter, including heuristic rules.
pub fn heuristic_linter() -> Linter {
    heuristic_linter_with_environment(default_environment())
}

/// Build the complete linter with an exact caller-supplied environment.
#[must_use]
/// Build the complete linter with an exact caller-supplied environment.
pub fn heuristic_linter_with_environment(environment: Environment) -> Linter {
    Linter::new(catalog(environment))
}

#[must_use]
/// Collect stable disclosure categories for findings in the JavaScript
/// namespace.
pub fn disclosures_for_report(report: &AnalysisReport) -> BTreeSet<&'static str> {
    report
        .files
        .iter()
        .flat_map(|file| file.findings.iter())
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

/// Browser, Node.js, and Electron globals used by the combined JavaScript
/// catalog.
#[must_use]
pub fn default_environment() -> Environment {
    let mut environment = Environment::default();
    environment
        .add_globals([
            "Buffer",
            "EventSource",
            "Notification",
            "URL",
            "URLSearchParams",
            "WebSocket",
            "XMLHttpRequest",
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
            "navigator",
            "process",
            "queueMicrotask",
            "require",
            "sessionStorage",
            "setImmediate",
            "setInterval",
            "setTimeout",
        ])
        .expect("built-in JavaScript environment names are valid");
    for name in ["window", "self", "global"] {
        environment
            .add_global_object(name)
            .expect("built-in JavaScript global-object names are valid");
    }
    environment
}

fn catalog(environment: Environment) -> RuleCatalog {
    // Construct one namespaced catalog so every public profile shares the same
    // rule metadata and environment, differing only in confidence filtering.
    RuleCatalog::with_environment("js", rules::all(), environment).unwrap()
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
        let environment = default_environment();
        assert!(environment.global_bindings().any(|name| name == "fetch"));
        assert!(environment.global_objects().any(|name| name == "window"));
    }

    #[test]
    fn caller_can_extend_the_default_global_objects() {
        let mut environment = default_environment();
        environment.add_global_object("activeWindow").unwrap();
        let report = heuristic_linter_with_environment(environment)
            .lint_snippet("activeWindow.fetch('/x')", "main.js")
            .unwrap();
        assert!(
            report.files[0]
                .findings
                .iter()
                .any(|finding| finding.rule_id.as_str() == "js:network.request")
        );
    }
}
