//! Node subprocess-module rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(MatcherDecl::import("child_process"))
        .declaration(MatcherDecl::import("node:child_process"))
        .declaration(MatcherDecl::import("worker_threads"))
        .declaration(MatcherDecl::import("node:worker_threads"))
        .declaration(MatcherDecl::import("cluster"))
        .declaration(MatcherDecl::import("node:cluster"))
        .declaration(MatcherDecl::package_import("node-pty"))
        .declaration(MatcherDecl::package_import("pty.js"))
        .declaration(MatcherDecl::package_import("execa"))
        .declaration(MatcherDecl::package_import("cross-spawn"))
        .declaration(MatcherDecl::package_import("shelljs"))
        .declaration(MatcherDecl::package_import("zx"))
        .declaration(MatcherDecl::package_import("npm-run-path"))
        .declaration(MatcherDecl::package_import("foreground-child"))
        .declaration(MatcherDecl::package_import("spawn-command"))
        .declaration(MatcherDecl::package_import("concurrently"))
        .declaration(MatcherDecl::package_import("npm-run-all"))
        .declaration(MatcherDecl::package_import("sudo-prompt"))
        .build()
        .unwrap()
}
