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
        .matcher(Matcher::package_import("@codemirror/state").unwrap())
        .matcher(Matcher::package_import("@codemirror/view").unwrap())
        .matcher(Matcher::package_import("@codemirror/language").unwrap())
        .matcher(Matcher::package_import("@codemirror/commands").unwrap())
        .matcher(Matcher::package_import("@codemirror/lang-markdown").unwrap())
        .matcher(Matcher::package_import("@codemirror/lang-javascript").unwrap())
        .matcher(Matcher::package_import("@codemirror/lang-json").unwrap())
        .matcher(Matcher::package_import("@codemirror/autocomplete").unwrap())
        .matcher(Matcher::package_import("@codemirror/lint").unwrap())
        .matcher(Matcher::package_import("@codemirror/search").unwrap())
        .matcher(Matcher::package_import("@codemirror/collab").unwrap())
        .matcher(Matcher::package_import("@lezer/common").unwrap())
        .matcher(Matcher::package_import("@lezer/highlight").unwrap())
        .matcher(Matcher::package_import("@lezer/lr").unwrap())
        .matcher(Matcher::package_import("@lezer/javascript").unwrap())
        .matcher(Matcher::package_import("@lezer/markdown").unwrap())
        .build()
        .unwrap()
}
