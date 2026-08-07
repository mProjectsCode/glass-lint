//! Browser notification-permission rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects unshadowed `Notification.requestPermission` calls, its rooted
/// `window.Notification` spelling, notification construction, and
/// service-worker `self.registration.showNotification`. Local host-shaped
/// objects and aliases reassigned to another function are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-notifications")
        .description("Requests browser notifications")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted(
            "Notification.requestPermission",
        ))
        .query(EventQuery::member_call_rooted(
            "self.registration.showNotification",
        ))
        .query(EventQuery::member_call_rooted(
            "registration.showNotification",
        ))
        .query(EventQuery::constructor_global("Notification"))
        .build()
        .unwrap()
}
