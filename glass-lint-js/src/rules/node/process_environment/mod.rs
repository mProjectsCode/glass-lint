//! Node process-environment rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

const PROCESS_READS: &[&str] = &[
    "process.env",
    "process.platform",
    "process.argv",
    "process.execPath",
    "process.arch",
    "process.version",
    "process.versions",
    "process.release",
    "process.pid",
    "process.ppid",
    "process.execArgv",
    "process.title",
    "process.config",
    "process.features",
    "process.report",
    "process.allowedNodeEnvironmentFlags",
    "process.debugPort",
    "process.sourceMapsEnabled",
];

const PROCESS_CALLS: &[&str] = &[
    "process.cwd",
    "process.memoryUsage",
    "process.resourceUsage",
    "process.cpuUsage",
    "process.uptime",
    "process.hrtime",
    "process.getActiveResourcesInfo",
    "process.constrainedMemory",
    "process.getuid",
    "process.geteuid",
    "process.getgid",
    "process.getegid",
    "process.getgroups",
    "process.umask",
    "process.getBuiltinModule",
    "process.loadEnvFile",
];

/// Detects rooted reads of Node's `process.env` and `process.platform`,
/// including direct member access and aliases that retain the rooted
/// provenance. Local or reassigned `process` aliases, unlisted properties,
/// and dynamic property names are excluded; the values read are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("node.process-environment")
        .description("Reads Node process environment or platform metadata")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .queries(
            PROCESS_READS
                .iter()
                .copied()
                .map(EventQuery::member_read_rooted),
        )
        .queries(
            PROCESS_CALLS
                .iter()
                .copied()
                .map(EventQuery::member_call_rooted),
        )
        .build()
        .unwrap()
}
