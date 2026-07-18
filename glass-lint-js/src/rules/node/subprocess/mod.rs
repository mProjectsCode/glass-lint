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
        .matcher(Matcher::import("node-pty"))
        .matcher(Matcher::import("pty.js"))
        .matcher(Matcher::import("execa"))
        .matcher(Matcher::import("cross-spawn"))
        .matcher(Matcher::import("shelljs"))
        .matcher(Matcher::import("zx"))
        .matcher(Matcher::import("npm-run-path"))
        .matcher(Matcher::import("foreground-child"))
        .matcher(Matcher::import("spawn-command"))
        .matcher(Matcher::import("concurrently"))
        .matcher(Matcher::import("npm-run-all"))
        .matcher(Matcher::import("sudo-prompt"))
        .build()
        .unwrap()
}
