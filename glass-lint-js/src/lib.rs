//! Generic JavaScript, browser, Node.js, and Electron rules.
//!
//! This crate owns the provider namespace, its default host environment, and
//! the recommended/heuristic catalog profiles while delegating matching to
//! core.

#[cfg(test)]
use glass_lint_core::project::SourceFile;
use glass_lint_core::{Environment, LinterConfig, RuleCatalog, RuleId, RuleMetadata};

mod rules;

/// JavaScript host target whose catalogs should be composed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JavaScriptTarget {
    Js,
    Browser,
    Node,
    Electron,
}

impl JavaScriptTarget {
    /// Build the catalog and host environment for this JavaScript target.
    #[must_use]
    pub fn config(self) -> LinterConfig {
        let (catalogs, environment) = match self {
            Self::Js => (vec![js_catalog()], js_environment()),
            Self::Browser => (vec![js_catalog(), browser_catalog()], browser_environment()),
            Self::Node => (vec![js_catalog(), node_catalog()], node_environment()),
            Self::Electron => (
                vec![
                    js_catalog(),
                    browser_catalog(),
                    node_catalog(),
                    electron_catalog(),
                ],
                electron_environment(),
            ),
        };
        LinterConfig::new(catalogs, environment)
    }

    /// Return whether a fully-qualified rule belongs to this target.
    #[must_use]
    pub fn accepts_rule(self, id: &RuleId) -> bool {
        let prefixes = match self {
            Self::Js => &["js:"][..],
            Self::Browser => &["js:", "browser:"],
            Self::Node => &["js:", "node:"],
            Self::Electron => &["js:", "browser:", "node:", "electron:"],
        };
        prefixes
            .iter()
            .any(|prefix| id.as_str().starts_with(prefix))
    }
}

/// Ordered JavaScript-provider catalogs and their target compositions.
pub struct JavaScriptCatalogBundle {
    catalogs: [RuleCatalog; 4],
}

impl JavaScriptCatalogBundle {
    #[must_use]
    pub fn new() -> Self {
        Self {
            catalogs: [
                js_catalog(),
                browser_catalog(),
                node_catalog(),
                electron_catalog(),
            ],
        }
    }

    /// Return isolated catalogs in their canonical documentation order.
    pub fn catalog_entries(&self) -> impl Iterator<Item = (&'static str, &RuleCatalog)> {
        ["js", "browser", "node", "electron"]
            .into_iter()
            .zip(self.catalogs.iter())
    }

    /// Return metadata grouped by isolated catalog in canonical order.
    pub fn metadata_by_catalog(&self) -> Vec<(&'static str, Vec<RuleMetadata>)> {
        self.catalog_entries()
            .map(|(name, catalog)| (name, catalog.metadata()))
            .collect()
    }

    /// Consume the bundle into the catalogs required by one host target.
    #[must_use]
    pub fn into_target(self, target: JavaScriptTarget) -> Vec<RuleCatalog> {
        let [js, browser, node, electron] = self.catalogs;
        match target {
            JavaScriptTarget::Js => vec![js],
            JavaScriptTarget::Browser => vec![js, browser],
            JavaScriptTarget::Node => vec![js, node],
            JavaScriptTarget::Electron => vec![js, browser, node, electron],
        }
    }

    /// Return flattened metadata in canonical catalog order.
    #[must_use]
    pub fn metadata(&self) -> Vec<RuleMetadata> {
        self.metadata_by_catalog()
            .into_iter()
            .flat_map(|(_, metadata)| metadata)
            .collect()
    }
}

impl Default for JavaScriptCatalogBundle {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
/// Return metadata for every rule in the `js:` provider catalog.
pub fn rule_metadata() -> Vec<RuleMetadata> {
    JavaScriptCatalogBundle::new().metadata()
}

#[must_use]
/// Return the isolated JavaScript catalog.
pub fn js_catalog() -> RuleCatalog {
    RuleCatalog::new("js", rules::js()).expect("valid JS catalog")
}
#[must_use]
/// Return the isolated browser catalog.
pub fn browser_catalog() -> RuleCatalog {
    RuleCatalog::new("browser", rules::browser()).expect("valid browser catalog")
}
#[must_use]
/// Return the isolated Electron catalog.
pub fn electron_catalog() -> RuleCatalog {
    RuleCatalog::new("electron", rules::electron()).expect("valid Electron catalog")
}
#[must_use]
/// Return the isolated Node.js catalog.
pub fn node_catalog() -> RuleCatalog {
    RuleCatalog::new("node", rules::node()).expect("valid Node catalog")
}

/// Return the complete core configuration for the plain JavaScript target.
#[must_use]
pub fn js_config() -> LinterConfig {
    JavaScriptTarget::Js.config()
}

/// Return the complete core configuration for the browser target.
#[must_use]
pub fn browser_config() -> LinterConfig {
    JavaScriptTarget::Browser.config()
}

/// Return the complete core configuration for the Node target.
#[must_use]
pub fn node_config() -> LinterConfig {
    JavaScriptTarget::Node.config()
}

/// Return the complete core configuration for the Electron target.
#[must_use]
pub fn electron_config() -> LinterConfig {
    JavaScriptTarget::Electron.config()
}

#[must_use]
/// Return the host-independent JavaScript environment.
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
        .add_global_object_with_members(
            "Atomics",
            [
                "add",
                "and",
                "compareExchange",
                "exchange",
                "isLockFree",
                "load",
                "notify",
                "or",
                "store",
                "sub",
                "wait",
                "waitAsync",
                "wake",
                "xor",
            ],
        )
        .expect("valid Atomics global object");
    environment
        .add_global_object_with_members(
            "WebAssembly",
            [
                "compile",
                "compileStreaming",
                "customSections",
                "instantiate",
                "instantiateStreaming",
                "imports",
                "Instance",
                "LinkError",
                "Memory",
                "Module",
                "CompileError",
                "Exception",
                "Global",
                "RuntimeError",
                "Table",
                "Tag",
                "validate",
            ],
        )
        .expect("valid WebAssembly global object");
    environment
}

#[must_use]
/// Return the browser and DOM environment layered over JavaScript globals.
pub fn browser_environment() -> Environment {
    let mut environment = js_environment();
    environment
        .add_globals([
            "EventSource",
            "Notification",
            "Worker",
            "SharedWorker",
            "BroadcastChannel",
            "MessageChannel",
            "MessagePort",
            "URL",
            "URLSearchParams",
            "WebSocket",
            "XMLHttpRequest",
            "addEventListener",
            "caches",
            "cookieStore",
            "document",
            "fetch",
            "importScripts",
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
    environment
        .add_global_object_with_members(
            "CSS",
            ["animationWorklet", "layoutWorklet", "paintWorklet"],
        )
        .expect("valid CSS global object");
    for name in ["window", "self"] {
        environment
            .add_global_object(name)
            .expect("valid browser global object");
    }
    environment
}

#[must_use]
/// Return the Node.js environment layered over JavaScript globals.
pub fn node_environment() -> Environment {
    let mut environment = js_environment();
    environment
        .add_globals([
            "Buffer",
            "crypto",
            "webcrypto",
            "module",
            "process",
            "require",
            "setImmediate",
            "clearImmediate",
            "Worker",
        ])
        .expect("valid Node globals");
    environment
        .add_global_object("global")
        .expect("valid Node global object");
    environment
}

#[must_use]
/// Return the Electron renderer environment layered over browser and Node.js.
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
mod tests;
