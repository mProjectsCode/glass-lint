use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects unshadowed `Notification.requestPermission` calls and aliases of
/// that browser API. A local `Notification` class and aliases reassigned to
/// another function are excluded.
pub(crate) fn rule() -> Rule {
    Rule::builder("browser.permissions-notifications")
        .label("Requests browser notifications")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call(
            "Notification.requestPermission",
        ))
        .build()
        .unwrap()
}
