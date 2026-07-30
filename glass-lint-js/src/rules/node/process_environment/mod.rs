//! Node process-environment rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects rooted reads of Node's `process.env` and `process.platform`,
/// including direct member access and aliases that retain the rooted
/// provenance. Local or reassigned `process` aliases, unlisted properties,
/// and dynamic property names are excluded; the values read are not analyzed.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("node.process-environment")
        .description("Reads Node process environment or platform metadata")
        .category(Category::new("node/process").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_read_rooted("process.env"))
        .query(EventQuery::member_read_rooted("process.platform"))
        .query(EventQuery::member_read_rooted("process.argv"))
        .query(EventQuery::member_read_rooted("process.execPath"))
        .query(EventQuery::member_read_rooted("process.arch"))
        .query(EventQuery::member_read_rooted("process.version"))
        .query(EventQuery::member_read_rooted("process.versions"))
        .query(EventQuery::member_read_rooted("process.release"))
        .query(EventQuery::member_read_rooted("process.pid"))
        .query(EventQuery::member_read_rooted("process.ppid"))
        .query(EventQuery::member_read_rooted("process.execArgv"))
        .query(EventQuery::member_read_rooted("process.title"))
        .query(EventQuery::member_read_rooted("process.config"))
        .query(EventQuery::member_read_rooted("process.features"))
        .query(EventQuery::member_read_rooted("process.report"))
        .query(EventQuery::member_read_rooted(
            "process.allowedNodeEnvironmentFlags",
        ))
        .query(EventQuery::member_read_rooted("process.debugPort"))
        .query(EventQuery::member_read_rooted("process.sourceMapsEnabled"))
        .query(EventQuery::member_call_rooted("process.cwd"))
        .query(EventQuery::member_call_rooted("process.memoryUsage"))
        .query(EventQuery::member_call_rooted("process.resourceUsage"))
        .query(EventQuery::member_call_rooted("process.cpuUsage"))
        .query(EventQuery::member_call_rooted("process.uptime"))
        .query(EventQuery::member_call_rooted("process.hrtime"))
        .query(EventQuery::member_call_rooted(
            "process.getActiveResourcesInfo",
        ))
        .query(EventQuery::member_call_rooted("process.constrainedMemory"))
        .query(EventQuery::member_call_rooted("process.getuid"))
        .query(EventQuery::member_call_rooted("process.geteuid"))
        .query(EventQuery::member_call_rooted("process.getgid"))
        .query(EventQuery::member_call_rooted("process.getegid"))
        .query(EventQuery::member_call_rooted("process.getgroups"))
        .query(EventQuery::member_call_rooted("process.umask"))
        .query(EventQuery::member_call_rooted("process.getBuiltinModule"))
        .query(EventQuery::member_call_rooted("process.loadEnvFile"))
        .build()
        .unwrap()
}
