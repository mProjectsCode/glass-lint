//! Node subprocess-module rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of Node's exact
/// `child_process` module names and configured subprocess packages. It reports
/// module loading rather than a particular spawn API, and excludes similar
/// modules and shadowed loaders.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("node.subprocess")
        .description("Starts Node subprocesses")
        .category(Category::new("node/process").unwrap())
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .query(EventQuery::import_exact("child_process"))
        .query(EventQuery::import_exact("node:child_process"))
        .query(EventQuery::import_exact("worker_threads"))
        .query(EventQuery::import_exact("node:worker_threads"))
        .query(EventQuery::import_exact("cluster"))
        .query(EventQuery::import_exact("node:cluster"))
        .query(EventQuery::import_package("node-pty"))
        .query(EventQuery::import_package("pty.js"))
        .query(EventQuery::import_package("execa"))
        .query(EventQuery::import_package("cross-spawn"))
        .query(EventQuery::import_package("shelljs"))
        .query(EventQuery::import_package("zx"))
        .query(EventQuery::import_package("npm-run-path"))
        .query(EventQuery::import_package("foreground-child"))
        .query(EventQuery::import_package("spawn-command"))
        .query(EventQuery::import_package("concurrently"))
        .query(EventQuery::import_package("npm-run-all"))
        .query(EventQuery::import_package("sudo-prompt"))
        .build()
        .unwrap()
}
