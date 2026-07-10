use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("browser.clipboard-write")
        .label("Writes clipboard data")
        .category("browser/clipboard")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("navigator.clipboard.write"))
        .matcher(Matcher::rooted_member_call("navigator.clipboard.writeText"))
        .build()
        .unwrap()
}
