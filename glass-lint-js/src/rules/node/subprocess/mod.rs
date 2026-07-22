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
        .matcher(Matcher::package_import("node-pty"))
        .matcher(Matcher::package_import("pty.js"))
        .matcher(Matcher::package_import("execa"))
        .matcher(Matcher::package_import("cross-spawn"))
        .matcher(Matcher::package_import("shelljs"))
        .matcher(Matcher::package_import("zx"))
        .matcher(Matcher::package_import("npm-run-path"))
        .matcher(Matcher::package_import("foreground-child"))
        .matcher(Matcher::package_import("spawn-command"))
        .matcher(Matcher::package_import("concurrently"))
        .matcher(Matcher::package_import("npm-run-all"))
        .matcher(Matcher::package_import("sudo-prompt"))
        .build()
        .unwrap()
}
