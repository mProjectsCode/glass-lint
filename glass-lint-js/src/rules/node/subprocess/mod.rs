//! Node subprocess-module rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

const SUBPROCESS_MODULES: &[&str] = &[
    "child_process",
    "node:child_process",
    "worker_threads",
    "node:worker_threads",
    "cluster",
    "node:cluster",
];
const SUBPROCESS_PACKAGES: &[&str] = &[
    "node-pty",
    "pty.js",
    "execa",
    "cross-spawn",
    "shelljs",
    "zx",
    "npm-run-path",
    "foreground-child",
    "spawn-command",
    "concurrently",
    "npm-run-all",
    "sudo-prompt",
];
const SUBPROCESS_MODULE_CALLS: &[(&str, &str)] = &[
    ("child_process", "spawn"),
    ("child_process", "exec"),
    ("child_process", "execFile"),
    ("child_process", "fork"),
    ("node:child_process", "spawn"),
    ("node:child_process", "exec"),
    ("node:child_process", "execFile"),
    ("node:child_process", "fork"),
    ("cluster", "fork"),
    ("node:cluster", "fork"),
];

/// Detects subprocess module indicators and calls to the principal
/// `child_process`, `cluster`, and `worker_threads` APIs. Imports alone remain
/// an indicator; operation calls are reported only with module provenance.
pub fn rule() -> Rule {
    Rule::builder("node.subprocess")
        .description("Starts Node subprocesses")
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .queries(
            SUBPROCESS_MODULES
                .iter()
                .copied()
                .map(EventQuery::import_exact),
        )
        .queries(
            SUBPROCESS_PACKAGES
                .iter()
                .copied()
                .map(EventQuery::import_package),
        )
        .queries(
            SUBPROCESS_MODULE_CALLS
                .iter()
                .copied()
                .map(|pair| EventQuery::member_call_module(pair.0, pair.1)),
        )
        .build()
        .unwrap()
}
