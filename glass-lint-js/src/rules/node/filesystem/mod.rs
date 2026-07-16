use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the exact Node
/// filesystem and path module names. The finding is attached to the module
/// load and does not infer later API use, local names, or similarly named
/// packages; shadowed loaders and non-listed modules are excluded.
pub fn rule() -> Rule {
    Rule::builder("node.filesystem")
        .label("Uses Node filesystem and path APIs")
        .category("node/filesystem")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::import("fs"))
        .matcher(Matcher::import("fs/promises"))
        .matcher(Matcher::import("node:fs"))
        .matcher(Matcher::import("node:fs/promises"))
        .matcher(Matcher::import("path"))
        .matcher(Matcher::import("node:path"))
        .build()
        .unwrap()
}
