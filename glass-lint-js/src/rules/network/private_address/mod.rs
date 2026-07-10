use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("network.private-address")
        .label("References private-network addresses")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::string_literal("localhost"))
        .matcher(Matcher::string_literal("127.0.0.1"))
        .matcher(Matcher::string_literal("0.0.0.0"))
        .matcher(Matcher::string_literal("http://192.168."))
        .matcher(Matcher::string_literal("https://192.168."))
        .matcher(Matcher::string_literal("http://10."))
        .matcher(Matcher::string_literal("https://10."))
        .build()
        .unwrap()
}
