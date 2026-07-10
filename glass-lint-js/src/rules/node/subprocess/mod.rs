use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("node.subprocess")
        .label("Starts Node subprocesses")
        .category("node/process")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .severity(Severity::Warning)
        .matcher(Matcher::import("child_process"))
        .matcher(Matcher::import("node:child_process"))
        .build()
        .unwrap()
}
