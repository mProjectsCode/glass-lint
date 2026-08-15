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
mod tests;
