use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("network.header-indicator")
        .label("References authorization or user-agent headers")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::string_literal("User-Agent"))
        .matcher(Matcher::string_literal("user-agent"))
        .matcher(Matcher::string_literal("Authorization"))
        .build()
        .unwrap()
}
