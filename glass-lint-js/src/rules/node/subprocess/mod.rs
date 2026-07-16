use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of Node's exact
/// `child_process` module names. It reports module loading rather than a
/// particular spawn API, and excludes similar modules and shadowed loaders.
pub fn rule() -> Rule {
    Rule::builder("node.subprocess")
        .label("Starts Node subprocesses")
        .category("node/process")
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .matcher(Matcher::import("child_process"))
        .matcher(Matcher::import("node:child_process"))
        .build()
        .unwrap()
}
