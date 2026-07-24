//! Generic JavaScript, browser, Node.js, and Electron rules.
//!
//! This crate owns the provider namespace, its default host environment, and
//! the recommended/heuristic catalog profiles while delegating matching to
//! core.

use std::collections::BTreeSet;

use glass_lint_core::{
    Environment, LinterConfig, RuleCatalog, RuleMetadata, project::AnalysisReport,
};

mod disclosures;
mod rules;

#[must_use]
/// Return metadata for every rule in the `js:` provider catalog.
pub fn rule_metadata() -> Vec<RuleMetadata> {
    [
        js_catalog(),
        browser_catalog(),
        electron_catalog(),
        node_catalog(),
    ]
    .into_iter()
    .flat_map(|catalog| catalog.metadata())
    .collect()
}

#[must_use]
pub fn js_catalog() -> RuleCatalog {
    RuleCatalog::new("js", rules::js()).expect("valid JS catalog")
}
#[must_use]
pub fn browser_catalog() -> RuleCatalog {
    RuleCatalog::new("browser", rules::browser()).expect("valid browser catalog")
}
#[must_use]
pub fn electron_catalog() -> RuleCatalog {
    RuleCatalog::new("electron", rules::electron()).expect("valid Electron catalog")
}
#[must_use]
pub fn node_catalog() -> RuleCatalog {
    RuleCatalog::new("node", rules::node()).expect("valid Node catalog")
}

/// Return the complete core configuration for the plain JavaScript target.
#[must_use]
pub fn js_config() -> LinterConfig {
    LinterConfig::new(vec![js_catalog()], js_environment())
}

/// Return the complete core configuration for the browser target.
#[must_use]
pub fn browser_config() -> LinterConfig {
    LinterConfig::new(vec![js_catalog(), browser_catalog()], browser_environment())
}

/// Return the complete core configuration for the Node target.
#[must_use]
pub fn node_config() -> LinterConfig {
    LinterConfig::new(vec![js_catalog(), node_catalog()], node_environment())
}

/// Return the complete core configuration for the Electron target.
#[must_use]
pub fn electron_config() -> LinterConfig {
    LinterConfig::new(
        vec![
            js_catalog(),
            browser_catalog(),
            node_catalog(),
            electron_catalog(),
        ],
        electron_environment(),
    )
}

const PROVIDER_PREFIXES: [&str; 4] = ["js:", "browser:", "node:", "electron:"];

#[must_use]
pub fn disclosures_for_report(report: &AnalysisReport) -> BTreeSet<&'static str> {
    report
        .files()
        .iter()
        .flat_map(|file| file.findings().iter())
        .flat_map(|finding| {
            let id = finding.rule_id().as_str();
            PROVIDER_PREFIXES
                .iter()
                .find_map(|prefix| id.strip_prefix(prefix))
                .into_iter()
                .flat_map(|unprefixed| disclosures::for_rule(unprefixed).iter().copied())
        })
        .collect()
}

#[must_use]
pub fn js_environment() -> Environment {
    let mut environment = Environment::default();
    environment
        .add_globals([
            "console",
            "eval",
            "queueMicrotask",
            "setTimeout",
            "setInterval",
            "clearTimeout",
            "clearInterval",
        ])
        .expect("valid JS globals");
    environment
}

#[must_use]
pub fn browser_environment() -> Environment {
    let mut environment = js_environment();
    environment
        .add_globals([
            "EventSource",
            "Notification",
            "URL",
            "URLSearchParams",
            "WebSocket",
            "XMLHttpRequest",
            "addEventListener",
            "caches",
            "cookieStore",
            "document",
            "fetch",
            "indexedDB",
            "localStorage",
            "navigator",
            "oncopy",
            "oncut",
            "onkeydown",
            "onkeypress",
            "onkeyup",
            "onpaste",
            "screen",
            "sessionStorage",
            "showDirectoryPicker",
            "showOpenFilePicker",
            "showSaveFilePicker",
        ])
        .expect("valid browser globals");
    for name in ["window", "self"] {
        environment
            .add_global_object(name)
            .expect("valid browser global object");
    }
    environment
}

#[must_use]
pub fn node_environment() -> Environment {
    let mut environment = js_environment();
    environment
        .add_globals([
            "Buffer",
            "crypto",
            "module",
            "process",
            "require",
            "setImmediate",
            "clearImmediate",
        ])
        .expect("valid Node globals");
    environment
        .add_global_object("global")
        .expect("valid Node global object");
    environment
}

#[must_use]
pub fn electron_environment() -> Environment {
    let mut environment = browser_environment();
    environment
        .add_globals([
            "Buffer",
            "crypto",
            "module",
            "process",
            "require",
            "setImmediate",
            "clearImmediate",
        ])
        .expect("valid Electron globals");
    environment
        .add_global_object("global")
        .expect("valid Electron global object");
    environment
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogs_are_namespaced() {
        assert!(
            js_catalog()
                .rule_ids()
                .iter()
                .all(|rule| rule.as_str().starts_with("js:"))
        );
        assert!(
            browser_catalog()
                .rule_ids()
                .iter()
                .all(|rule| rule.as_str().starts_with("browser:"))
        );
        assert!(
            electron_catalog()
                .rule_ids()
                .iter()
                .all(|rule| rule.as_str().starts_with("electron:"))
        );
        assert!(
            node_catalog()
                .rule_ids()
                .iter()
                .all(|rule| rule.as_str().starts_with("node:"))
        );
        let environment = electron_environment();
        assert!(environment.global_bindings().any(|name| name == "fetch"));
        assert!(environment.global_objects().any(|name| name == "window"));
    }

    #[test]
    fn caller_can_extend_the_electron_environment() {
        let mut environment = electron_environment();
        environment.add_global_object("activeWindow").unwrap();
        let linter = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
            vec![js_catalog(), browser_catalog()],
            environment,
        ))
        .unwrap();
        let report = linter
            .lint_snippet("activeWindow.fetch('/x')", "main.js")
            .unwrap();
        assert!(
            report.files()[0]
                .findings()
                .iter()
                .any(|finding| finding.rule_id().as_str() == "browser:network.request")
        );
    }

    #[test]
    fn node_web_crypto_global_is_rooted() {
        let linter = glass_lint_core::Linter::new(node_config()).unwrap();
        let report = linter
            .lint_snippet("crypto.subtle.digest('SHA-256', bytes)", "main.js")
            .unwrap();
        assert!(
            report.files()[0]
                .findings()
                .iter()
                .any(|finding| finding.rule_id().as_str() == "node:crypto.operation")
        );
    }

    #[test]
    fn node_web_crypto_global_survives_catalog_imports() {
        let linter = glass_lint_core::Linter::new(node_config()).unwrap();
        let report = linter
            .lint_snippet(
                "import c from 'node:crypto'; import * as cryptoPromises from 'crypto/promises'; import * as nodeCryptoPromises from 'node:crypto/promises'; import coreCrypto from 'crypto'; import cryptoJs from 'crypto-js'; crypto.subtle.digest('SHA-256', bytes);",
                "main.js",
            )
            .unwrap();
        assert!(
            report.files()[0]
                .findings()
                .iter()
                .any(|finding| { finding.rule_id().as_str() == "node:crypto.operation" })
        );
    }

    #[test]
    fn node_crypto_fixture_uses_rooted_web_crypto() {
        let linter = glass_lint_core::Linter::new(node_config()).unwrap();
        let source = include_str!("rules/node/crypto_operation/positive.js");
        let report = linter.lint_snippet(source, "positive.js").unwrap();
        let count = report.files()[0]
            .findings()
            .iter()
            .filter(|finding| finding.rule_id().as_str() == "node:crypto.operation")
            .count();
        assert!(count >= 29, "expected rooted calls in fixture, got {count}");
    }
}
