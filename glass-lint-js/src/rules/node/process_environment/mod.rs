//! Node process-environment rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

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
        .matcher(Matcher::rooted_member_read("process.env"))
        .matcher(Matcher::rooted_member_read("process.platform"))
        .matcher(Matcher::rooted_member_read("process.argv"))
        .matcher(Matcher::rooted_member_read("process.execPath"))
        .matcher(Matcher::rooted_member_read("process.arch"))
        .matcher(Matcher::rooted_member_read("process.version"))
        .matcher(Matcher::rooted_member_read("process.versions"))
        .matcher(Matcher::rooted_member_read("process.release"))
        .matcher(Matcher::rooted_member_read("process.pid"))
        .matcher(Matcher::rooted_member_read("process.ppid"))
        .matcher(Matcher::rooted_member_read("process.execArgv"))
        .matcher(Matcher::rooted_member_read("process.title"))
        .matcher(Matcher::rooted_member_read("process.config"))
        .matcher(Matcher::rooted_member_read("process.features"))
        .matcher(Matcher::rooted_member_read("process.report"))
        .matcher(Matcher::rooted_member_read(
            "process.allowedNodeEnvironmentFlags",
        ))
        .matcher(Matcher::rooted_member_read("process.debugPort"))
        .matcher(Matcher::rooted_member_read("process.sourceMapsEnabled"))
        .matcher(Matcher::rooted_member_call("process.cwd"))
        .matcher(Matcher::rooted_member_call("process.memoryUsage"))
        .matcher(Matcher::rooted_member_call("process.resourceUsage"))
        .matcher(Matcher::rooted_member_call("process.cpuUsage"))
        .matcher(Matcher::rooted_member_call("process.uptime"))
        .matcher(Matcher::rooted_member_call("process.hrtime"))
        .matcher(Matcher::rooted_member_call(
            "process.getActiveResourcesInfo",
        ))
        .matcher(Matcher::rooted_member_call("process.constrainedMemory"))
        .matcher(Matcher::rooted_member_call("process.getuid"))
        .matcher(Matcher::rooted_member_call("process.geteuid"))
        .matcher(Matcher::rooted_member_call("process.getgid"))
        .matcher(Matcher::rooted_member_call("process.getegid"))
        .matcher(Matcher::rooted_member_call("process.getgroups"))
        .matcher(Matcher::rooted_member_call("process.umask"))
        .matcher(Matcher::rooted_member_call("process.getBuiltinModule"))
        .matcher(Matcher::rooted_member_call("process.loadEnvFile"))
        .matcher(Matcher::rooted_member_read("global.process.env"))
        .matcher(Matcher::rooted_member_read("global.process.platform"))
        .matcher(Matcher::rooted_member_read("global.process.argv"))
        .matcher(Matcher::rooted_member_read("global.process.execPath"))
        .matcher(Matcher::rooted_member_read("global.process.version"))
        .matcher(Matcher::rooted_member_read("global.process.versions"))
        .matcher(Matcher::rooted_member_read("global.process.release"))
        .matcher(Matcher::rooted_member_call("global.process.cwd"))
        .matcher(Matcher::rooted_member_call("global.process.memoryUsage"))
        .matcher(Matcher::rooted_member_call("global.process.resourceUsage"))
        .matcher(Matcher::rooted_member_call("global.process.uptime"))
        .matcher(Matcher::rooted_member_call("global.process.getuid"))
        .matcher(Matcher::rooted_member_read("globalThis.process.env"))
        .matcher(Matcher::rooted_member_read("globalThis.process.platform"))
        .matcher(Matcher::rooted_member_read("globalThis.process.argv"))
        .matcher(Matcher::rooted_member_read("globalThis.process.execPath"))
        .matcher(Matcher::rooted_member_read("globalThis.process.version"))
        .matcher(Matcher::rooted_member_read("globalThis.process.versions"))
        .matcher(Matcher::rooted_member_read("globalThis.process.release"))
        .matcher(Matcher::rooted_member_call("globalThis.process.cwd"))
        .matcher(Matcher::rooted_member_call(
            "globalThis.process.memoryUsage",
        ))
        .matcher(Matcher::rooted_member_call(
            "globalThis.process.resourceUsage",
        ))
        .matcher(Matcher::rooted_member_call("globalThis.process.uptime"))
        .matcher(Matcher::rooted_member_call("globalThis.process.getuid"))
        .build()
        .unwrap()
}
