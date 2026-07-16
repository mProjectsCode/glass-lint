use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects static ESM imports and unshadowed CommonJS loads of the exact
/// CodeMirror packages used by the provider. The finding is attached to the
/// module load, not later API use; similar package names, dynamic module names,
/// and shadowed `require` loaders are excluded by module provenance.
pub fn rule() -> Rule {
    Rule::builder("codemirror.extension")
        .label("Uses CodeMirror extension primitives")
        .category("codemirror")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::import("@codemirror/state"))
        .matcher(Matcher::import("@codemirror/view"))
        .matcher(Matcher::import("@codemirror/language"))
        .matcher(Matcher::import("@codemirror/commands"))
        .build()
        .unwrap()
}
