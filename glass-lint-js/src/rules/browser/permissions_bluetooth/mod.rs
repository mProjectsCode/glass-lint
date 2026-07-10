use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("browser.permissions-bluetooth")
        .label("Uses browser Bluetooth")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call(
            "navigator.bluetooth.requestDevice",
        ))
        .build()
        .unwrap()
}
