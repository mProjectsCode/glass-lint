//! Node subprocess-module rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

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
        .query(QueryDecl::import_exact("child_process"))
        .query(QueryDecl::import_exact("node:child_process"))
        .query(QueryDecl::import_exact("worker_threads"))
        .query(QueryDecl::import_exact("node:worker_threads"))
        .query(QueryDecl::import_exact("cluster"))
        .query(QueryDecl::import_exact("node:cluster"))
        .query(QueryDecl::import_package("node-pty"))
        .query(QueryDecl::import_package("pty.js"))
        .query(QueryDecl::import_package("execa"))
        .query(QueryDecl::import_package("cross-spawn"))
        .query(QueryDecl::import_package("shelljs"))
        .query(QueryDecl::import_package("zx"))
        .query(QueryDecl::import_package("npm-run-path"))
        .query(QueryDecl::import_package("foreground-child"))
        .query(QueryDecl::import_package("spawn-command"))
        .query(QueryDecl::import_package("concurrently"))
        .query(QueryDecl::import_package("npm-run-all"))
        .query(QueryDecl::import_package("sudo-prompt"))
        .build()
        .unwrap()
}
