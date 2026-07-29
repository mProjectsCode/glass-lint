//! Node process-environment rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

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
        .query(QueryDecl::member_read_rooted("process.env"))
        .query(QueryDecl::member_read_rooted("process.platform"))
        .query(QueryDecl::member_read_rooted("process.argv"))
        .query(QueryDecl::member_read_rooted("process.execPath"))
        .query(QueryDecl::member_read_rooted("process.arch"))
        .query(QueryDecl::member_read_rooted("process.version"))
        .query(QueryDecl::member_read_rooted("process.versions"))
        .query(QueryDecl::member_read_rooted("process.release"))
        .query(QueryDecl::member_read_rooted("process.pid"))
        .query(QueryDecl::member_read_rooted("process.ppid"))
        .query(QueryDecl::member_read_rooted("process.execArgv"))
        .query(QueryDecl::member_read_rooted("process.title"))
        .query(QueryDecl::member_read_rooted("process.config"))
        .query(QueryDecl::member_read_rooted("process.features"))
        .query(QueryDecl::member_read_rooted("process.report"))
        .query(QueryDecl::member_read_rooted(
            "process.allowedNodeEnvironmentFlags",
        ))
        .query(QueryDecl::member_read_rooted("process.debugPort"))
        .query(QueryDecl::member_read_rooted("process.sourceMapsEnabled"))
        .query(QueryDecl::member_call_rooted("process.cwd"))
        .query(QueryDecl::member_call_rooted("process.memoryUsage"))
        .query(QueryDecl::member_call_rooted("process.resourceUsage"))
        .query(QueryDecl::member_call_rooted("process.cpuUsage"))
        .query(QueryDecl::member_call_rooted("process.uptime"))
        .query(QueryDecl::member_call_rooted("process.hrtime"))
        .query(QueryDecl::member_call_rooted(
            "process.getActiveResourcesInfo",
        ))
        .query(QueryDecl::member_call_rooted("process.constrainedMemory"))
        .query(QueryDecl::member_call_rooted("process.getuid"))
        .query(QueryDecl::member_call_rooted("process.geteuid"))
        .query(QueryDecl::member_call_rooted("process.getgid"))
        .query(QueryDecl::member_call_rooted("process.getegid"))
        .query(QueryDecl::member_call_rooted("process.getgroups"))
        .query(QueryDecl::member_call_rooted("process.umask"))
        .query(QueryDecl::member_call_rooted("process.getBuiltinModule"))
        .query(QueryDecl::member_call_rooted("process.loadEnvFile"))
        .build()
        .unwrap()
}
