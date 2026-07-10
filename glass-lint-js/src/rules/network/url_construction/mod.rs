use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("network.url-construction")
        .label("Constructs or references URLs")
        .category("language/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::global_constructor("URL"))
        .matcher(Matcher::global_constructor("URLSearchParams"))
        .build()
        .unwrap()
}
