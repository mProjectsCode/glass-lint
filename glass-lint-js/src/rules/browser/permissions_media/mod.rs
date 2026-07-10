use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("browser.permissions-media")
        .label("Uses browser media capture")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call(
            "navigator.mediaDevices.getUserMedia",
        ))
        .build()
        .unwrap()
}
