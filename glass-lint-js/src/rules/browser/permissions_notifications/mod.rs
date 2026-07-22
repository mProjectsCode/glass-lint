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
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("Notification.requestPermission")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("self.registration.showNotification")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .constructor_global("Notification")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
