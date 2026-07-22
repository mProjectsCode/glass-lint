//! Browser notification-permission rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(MatcherDecl::rooted_member_call(
            "Notification.requestPermission",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "self.registration.showNotification",
        ))
        .declaration(MatcherDecl::global_constructor("Notification"))
        .build()
        .unwrap()
}
