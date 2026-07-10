use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted reads of Node's `process.env` and `process.platform`,
/// including direct member access and aliases that retain the rooted
/// provenance. Local or reassigned `process` aliases, unlisted properties,
/// and dynamic property names are excluded; the values read are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("node.process-environment")
        .label("Reads Node process environment or platform metadata")
        .category("node/process")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_read("process.env"))
        .matcher(Matcher::rooted_member_read("process.platform"))
        .build()
        .unwrap()
}
