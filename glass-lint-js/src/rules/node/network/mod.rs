use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the exact Node `http`,
/// `https`, `node:http`, and `node:https` modules. It reports the module load
/// itself, not later API use, and relies on module provenance so similar names
/// and shadowed `require` bindings are excluded.
pub(crate) fn rule() -> Rule {
    Rule::builder("node.network")
        .label("Uses Node HTTP modules")
        .category("node/network")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::import("http"))
        .matcher(Matcher::import("https"))
        .matcher(Matcher::import("node:http"))
        .matcher(Matcher::import("node:https"))
        .build()
        .unwrap()
}
