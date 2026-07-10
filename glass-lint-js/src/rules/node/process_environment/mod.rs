use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

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
