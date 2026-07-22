//! CodeMirror extension module rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects static ESM imports and unshadowed CommonJS loads of the exact
/// CodeMirror packages used by the provider. The finding is attached to the
/// module load, not later API use; similar package names, dynamic module names,
/// and shadowed `require` loaders are excluded by module provenance.
pub fn rule() -> Rule {
    Rule::builder("codemirror.extension")
        .description("Uses CodeMirror extension primitives")
        .category("codemirror")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::package_import("@codemirror/state"))
        .matcher(Matcher::package_import("@codemirror/view"))
        .matcher(Matcher::package_import("@codemirror/language"))
        .matcher(Matcher::package_import("@codemirror/commands"))
        .matcher(Matcher::package_import("@codemirror/lang-markdown"))
        .matcher(Matcher::package_import("@codemirror/lang-javascript"))
        .matcher(Matcher::package_import("@codemirror/lang-json"))
        .matcher(Matcher::package_import("@codemirror/autocomplete"))
        .matcher(Matcher::package_import("@codemirror/lint"))
        .matcher(Matcher::package_import("@codemirror/search"))
        .matcher(Matcher::package_import("@codemirror/collab"))
        .matcher(Matcher::package_import("@lezer/common"))
        .matcher(Matcher::package_import("@lezer/highlight"))
        .matcher(Matcher::package_import("@lezer/lr"))
        .matcher(Matcher::package_import("@lezer/javascript"))
        .matcher(Matcher::package_import("@lezer/markdown"))
        .build()
        .unwrap()
}
