use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("node.filesystem")
        .label("Uses Node filesystem and path APIs")
        .category("node/filesystem")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::import("fs"))
        .matcher(Matcher::import("fs/promises"))
        .matcher(Matcher::import("node:fs"))
        .matcher(Matcher::import("node:fs/promises"))
        .matcher(Matcher::import("path"))
        .matcher(Matcher::import("node:path"))
        .build()
        .unwrap()
}
