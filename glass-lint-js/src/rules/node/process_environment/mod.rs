//! Node process-environment rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted reads of Node's `process.env` and `process.platform`,
/// including direct member access and aliases that retain the rooted
/// provenance. Local or reassigned `process` aliases, unlisted properties,
/// and dynamic property names are excluded; the values read are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("node.process-environment")
        .description("Reads Node process environment or platform metadata")
        .category("node/process")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_read("process.env"))
        .declaration(MatcherDecl::rooted_member_read("process.platform"))
        .declaration(MatcherDecl::rooted_member_read("process.argv"))
        .declaration(MatcherDecl::rooted_member_read("process.execPath"))
        .declaration(MatcherDecl::rooted_member_read("process.arch"))
        .declaration(MatcherDecl::rooted_member_read("process.version"))
        .declaration(MatcherDecl::rooted_member_read("process.versions"))
        .declaration(MatcherDecl::rooted_member_read("process.release"))
        .declaration(MatcherDecl::rooted_member_read("process.pid"))
        .declaration(MatcherDecl::rooted_member_read("process.ppid"))
        .declaration(MatcherDecl::rooted_member_read("process.execArgv"))
        .declaration(MatcherDecl::rooted_member_read("process.title"))
        .declaration(MatcherDecl::rooted_member_read("process.config"))
        .declaration(MatcherDecl::rooted_member_read("process.features"))
        .declaration(MatcherDecl::rooted_member_read("process.report"))
        .declaration(MatcherDecl::rooted_member_read(
            "process.allowedNodeEnvironmentFlags",
        ))
        .declaration(MatcherDecl::rooted_member_read("process.debugPort"))
        .declaration(MatcherDecl::rooted_member_read("process.sourceMapsEnabled"))
        .declaration(MatcherDecl::rooted_member_call("process.cwd"))
        .declaration(MatcherDecl::rooted_member_call("process.memoryUsage"))
        .declaration(MatcherDecl::rooted_member_call("process.resourceUsage"))
        .declaration(MatcherDecl::rooted_member_call("process.cpuUsage"))
        .declaration(MatcherDecl::rooted_member_call("process.uptime"))
        .declaration(MatcherDecl::rooted_member_call("process.hrtime"))
        .declaration(MatcherDecl::rooted_member_call(
            "process.getActiveResourcesInfo",
        ))
        .declaration(MatcherDecl::rooted_member_call("process.constrainedMemory"))
        .declaration(MatcherDecl::rooted_member_call("process.getuid"))
        .declaration(MatcherDecl::rooted_member_call("process.geteuid"))
        .declaration(MatcherDecl::rooted_member_call("process.getgid"))
        .declaration(MatcherDecl::rooted_member_call("process.getegid"))
        .declaration(MatcherDecl::rooted_member_call("process.getgroups"))
        .declaration(MatcherDecl::rooted_member_call("process.umask"))
        .declaration(MatcherDecl::rooted_member_call("process.getBuiltinModule"))
        .declaration(MatcherDecl::rooted_member_call("process.loadEnvFile"))
        .build()
        .unwrap()
}
