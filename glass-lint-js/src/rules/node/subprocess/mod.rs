//! Node subprocess-module rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of Node's exact
/// `child_process` module names and configured subprocess packages. It reports
/// module loading rather than a particular spawn API, and excludes similar
/// modules and shadowed loaders.
pub fn rule() -> Rule {
    Rule::builder("node.subprocess")
        .description("Starts Node subprocesses")
        .category("node/process")
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .matcher(Matcher::import("child_process"))
        .matcher(Matcher::import("node:child_process"))
        .matcher(Matcher::import("worker_threads"))
        .matcher(Matcher::import("node:worker_threads"))
        .matcher(Matcher::import("cluster"))
        .matcher(Matcher::import("node:cluster"))
        .matcher(Matcher::package_import("node-pty").unwrap())
        .matcher(Matcher::package_import("pty.js").unwrap())
        .matcher(Matcher::package_import("execa").unwrap())
        .matcher(Matcher::package_import("cross-spawn").unwrap())
        .matcher(Matcher::package_import("shelljs").unwrap())
        .matcher(Matcher::package_import("zx").unwrap())
        .matcher(Matcher::package_import("npm-run-path").unwrap())
        .matcher(Matcher::package_import("foreground-child").unwrap())
        .matcher(Matcher::package_import("spawn-command").unwrap())
        .matcher(Matcher::package_import("concurrently").unwrap())
        .matcher(Matcher::package_import("npm-run-all").unwrap())
        .matcher(Matcher::package_import("sudo-prompt").unwrap())
        .build()
        .unwrap()
}
