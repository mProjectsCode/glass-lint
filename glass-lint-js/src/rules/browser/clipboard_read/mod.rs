use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("browser.clipboard-read")
        .label("Reads clipboard data")
        .category("browser/clipboard")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("navigator.clipboard.read"))
        .matcher(Matcher::rooted_member_call("navigator.clipboard.readText"))
        .build()
        .unwrap()
}
