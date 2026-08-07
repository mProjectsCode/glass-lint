//! Obsidian rule definitions and ready-to-use core linters.
//!
//! The provider owns Obsidian globals and rule profiles; matching and report
//! primitives remain in the provider-neutral core crate.

#[cfg(test)]
use glass_lint_core::project::SourceFile;
use glass_lint_core::{Environment, LinterConfig, RuleCatalog, RuleId, RuleMetadata};

pub mod api_manifest;
mod catalog;
mod rules;

/// Ordered catalogs for the complete Obsidian renderer target.
pub struct ObsidianCatalogBundle {
    javascript: glass_lint_js::JavaScriptCatalogBundle,
    obsidian: RuleCatalog,
}

impl ObsidianCatalogBundle {
    #[must_use]
    pub fn new() -> Self {
        Self {
            javascript: glass_lint_js::JavaScriptCatalogBundle::new(),
            obsidian: obsidian_catalog(),
        }
    }

    /// Return all provider catalogs in the canonical target and documentation
    /// order.
    pub fn metadata_by_catalog(&self) -> Vec<(&'static str, Vec<RuleMetadata>)> {
        let mut metadata = self.javascript.metadata_by_catalog();
        metadata.push(("obsidian", self.obsidian.metadata()));
        metadata
    }

    #[must_use]
    pub fn metadata(&self) -> Vec<RuleMetadata> {
        self.metadata_by_catalog()
            .into_iter()
            .flat_map(|(_, metadata)| metadata)
            .collect()
    }

    #[must_use]
    pub fn into_catalogs(self) -> Vec<RuleCatalog> {
        let mut catalogs = self
            .javascript
            .into_target(glass_lint_js::JavaScriptTarget::Electron);
        catalogs.push(self.obsidian);
        catalogs
    }
}

impl Default for ObsidianCatalogBundle {
    fn default() -> Self {
        Self::new()
    }
}

/// Version-pinned runtime source for the plugin-manager API rules.
pub const PLUGIN_API_SOURCE: &str = "obsidian-runtime-plugin-manager@2026-07-31";

#[must_use]
/// Return metadata for every rule in the `obsidian:` provider catalog.
pub fn rule_metadata() -> Vec<RuleMetadata> {
    obsidian_catalog().metadata()
}

#[must_use]
/// Return the isolated Obsidian rule catalog.
pub fn obsidian_catalog() -> RuleCatalog {
    RuleCatalog::new("obsidian", catalog::obsidian_rules().to_vec())
        .expect("valid Obsidian catalog")
}

/// Return whether a fully-qualified rule belongs to the complete Obsidian
/// renderer target, including its JavaScript host catalogs.
#[must_use]
pub fn accepts_rule(id: &RuleId) -> bool {
    ["js:", "browser:", "node:", "electron:", "obsidian:"]
        .iter()
        .any(|prefix| id.as_str().starts_with(prefix))
}

/// Return whether a rule belongs to the isolated Obsidian catalog.
#[must_use]
pub fn accepts_isolated_rule(id: &RuleId) -> bool {
    id.as_str().starts_with("obsidian:")
}

#[must_use]
/// Return the complete Obsidian renderer environment.
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
        .add_global_object("Vault")
        .expect("valid Obsidian Vault global object");
    environment
}

/// Return the complete core configuration for the Obsidian renderer target.
#[must_use]
pub fn obsidian_config() -> LinterConfig {
    LinterConfig::new(
        ObsidianCatalogBundle::new().into_catalogs(),
        obsidian_environment(),
    )
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
        assert!(
            catalog
                .iter()
                .any(|rule| rule.id.as_str() == "obsidian:plugins.access")
        );
        assert!(
            catalog
                .iter()
                .any(|rule| rule.id.as_str() == "obsidian:plugins.enable-disable")
        );
        assert!(
            catalog
                .iter()
                .any(|rule| rule.id.as_str() == "obsidian:plugins.load-unload")
        );
        assert_eq!(
            PLUGIN_API_SOURCE,
            "obsidian-runtime-plugin-manager@2026-07-31"
        );
        assert_eq!(api_manifest::PUBLIC_API.version, "obsidian-api@2026-07-31");
        assert!(!api_manifest::PUBLIC_API.vault_events.contains(&"closed"));
        assert!(
            !api_manifest::PUBLIC_API
                .metadata_events
                .contains(&"finished")
        );
        assert!(
            !api_manifest::PUBLIC_API
                .top_level_helpers
                .contains(&"parseSubpath")
        );
    }

    #[test]
    fn catalog_bundle_preserves_provider_order() {
        let names: Vec<_> = ObsidianCatalogBundle::new()
            .metadata_by_catalog()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, ["js", "browser", "node", "electron", "obsidian"]);
    }

    #[test]
    fn active_window_is_a_configured_global_object() {
        use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

        let rule = Rule::builder("test.eval")
            .description("eval")
            .category(Category::new("test").unwrap())
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .query(EventQuery::call_global("eval"))
            .build()
            .unwrap();
        let report = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
            vec![RuleCatalog::new("test", vec![rule]).unwrap()],
            obsidian_environment(),
        ))
        .unwrap()
        .lint_source(SourceFile::new("main.js", "activeWindow.eval('x')").unwrap())
        .unwrap();
        assert_eq!(report.files()[0].findings().len(), 1);
    }

    #[test]
    fn active_window_shares_the_configured_environment() {
        use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

        let rule = Rule::builder("test.request")
            .description("request")
            .category(Category::new("test").unwrap())
            .severity(Severity::Info)
            .confidence(Confidence::High)
            .query(EventQuery::call_global("requestUrl"))
            .build()
            .unwrap();
        let report = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
            vec![RuleCatalog::new("test", vec![rule]).unwrap()],
            obsidian_environment(),
        ))
        .unwrap()
        .lint_source(
            SourceFile::new(
                "main.js",
                "requestUrl('/a'); window.requestUrl('/b'); activeWindow.requestUrl('/c');",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(report.files()[0].findings().len(), 3);
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
        .lint_source(
            SourceFile::new(
                "main.js",
                "import { request } from 'obsidian';\nrequest('/one');\nrequest('/two');",
            )
            .unwrap(),
        )
        .unwrap();
        let findings: Vec<_> = report.files()[0]
            .findings()
            .iter()
            .filter(|finding| finding.rule_id().as_str() == "obsidian:network.request")
            .collect();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].location().range().start().line(), 2);
        assert_eq!(findings[1].location().range().start().line(), 3);
    }

    #[test]
    fn default_config_reports_plugin_manager_usage() {
        let report = glass_lint_core::Linter::new(obsidian_config())
            .unwrap()
            .lint_source(SourceFile::new("main.js", "app.plugins.getPlugin('dataview');").unwrap())
            .unwrap();
        assert!(
            report.files()[0]
                .findings()
                .iter()
                .any(|finding| { finding.rule_id().as_str() == "obsidian:plugins.access" })
        );
    }
}
