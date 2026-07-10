use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("browser.environment")
        .label("Reads browser environment data")
        .category("browser/environment")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::heuristic_member_read("navigator.userAgent"))
        .matcher(Matcher::heuristic_member_read("navigator.platform"))
        .matcher(Matcher::heuristic_member_read("navigator.language"))
        .matcher(Matcher::heuristic_member_read("screen.width"))
        .matcher(Matcher::heuristic_member_read("screen.height"))
        .build()
        .unwrap()
}
