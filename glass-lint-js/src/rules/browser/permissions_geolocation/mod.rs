use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

pub(crate) fn rule() -> Rule {
    Rule::builder("browser.permissions-geolocation")
        .label("Uses browser geolocation")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call(
            "navigator.geolocation.getCurrentPosition",
        ))
        .build()
        .unwrap()
}
