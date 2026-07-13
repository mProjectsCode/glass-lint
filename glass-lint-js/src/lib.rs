//! Generic JavaScript, browser, Node.js, and Electron rules.

use glass_lint_core::{Environment, LintReport, Linter, RuleCatalog, RuleId, RuleMetadata};
use std::collections::BTreeSet;

mod disclosures;
mod rules;

pub fn rule_catalog() -> Vec<RuleMetadata> {
    catalog(default_environment()).metadata()
}

pub fn recommended_linter() -> Linter {
    recommended_linter_with_environment(default_environment())
}

/// Build the recommended linter with an exact caller-supplied environment.
pub fn recommended_linter_with_environment(environment: Environment) -> Linter {
    let catalog = catalog(environment);
    let enabled = rules::all()
        .into_iter()
        .filter(|rule| rule.confidence() == glass_lint_core::rules::Confidence::High)
        .map(|rule| RuleId::parse(format!("js:{}", rule.id())).unwrap());
    Linter::with_rules(catalog, enabled).unwrap()
}

pub fn heuristic_linter() -> Linter {
    heuristic_linter_with_environment(default_environment())
}

/// Build the complete linter with an exact caller-supplied environment.
pub fn heuristic_linter_with_environment(environment: Environment) -> Linter {
    Linter::new(catalog(environment))
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

/// Browser, Node.js, and Electron globals used by the combined JavaScript catalog.
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
            .lint("activeWindow.fetch('/x')", "main.js");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.rule_id.as_str() == "js:network.request")
        );
    }
}
