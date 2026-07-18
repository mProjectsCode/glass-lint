//! Browser notification-permission rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects unshadowed `Notification.requestPermission` calls, its rooted
/// `window.Notification` spelling, notification construction, and
/// service-worker `self.registration.showNotification`. Local host-shaped
/// objects and aliases reassigned to another function are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-notifications")
        .description("Requests browser notifications")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call(
            "Notification.requestPermission",
        ))
        .matcher(Matcher::rooted_member_call(
            "window.Notification.requestPermission",
        ))
        .matcher(Matcher::rooted_member_call(
            "globalThis.Notification.requestPermission",
        ))
        .matcher(Matcher::rooted_member_call(
            "self.registration.showNotification",
        ))
        .matcher(Matcher::global_constructor("Notification"))
        .build()
        .unwrap()
}
