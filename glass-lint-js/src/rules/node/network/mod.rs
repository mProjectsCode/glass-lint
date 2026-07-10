use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("node.network")
        .label("Uses Node HTTP modules")
        .category("node/network")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::import("http"))
        .matcher(Matcher::import("https"))
        .matcher(Matcher::import("node:http"))
        .matcher(Matcher::import("node:https"))
        .build()
        .unwrap()
}
