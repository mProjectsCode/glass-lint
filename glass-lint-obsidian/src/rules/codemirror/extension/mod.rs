//! CodeMirror extension module rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects static ESM imports and unshadowed CommonJS loads of the exact
/// CodeMirror packages used by the provider. The finding is attached to the
/// module load, not later API use; similar package names, dynamic module names,
/// and shadowed `require` loaders are excluded by module provenance.
const CODEMIRROR_PACKAGES: &[&str] = &[
    "@codemirror/state",
    "@codemirror/view",
    "@codemirror/language",
    "@codemirror/commands",
    "@codemirror/lang-markdown",
    "@codemirror/lang-javascript",
    "@codemirror/lang-json",
    "@codemirror/autocomplete",
    "@codemirror/lint",
    "@codemirror/search",
    "@codemirror/collab",
    "@lezer/common",
    "@lezer/highlight",
    "@lezer/lr",
    "@lezer/javascript",
    "@lezer/markdown",
];

pub fn rule() -> Rule {
    Rule::builder("codemirror.extension")
        .description("Uses CodeMirror extension primitives")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .queries(
            CODEMIRROR_PACKAGES
                .iter()
                .copied()
                .map(EventQuery::import_package),
        )
        .build()
        .unwrap()
}
