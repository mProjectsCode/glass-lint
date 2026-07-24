//! Browser hardware-permission rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects unshadowed WebHID, Web Serial, and WebUSB device requests. Rooted
/// aliases and static computed properties retain browser provenance; local
/// lookalikes, reassigned aliases, and dynamic properties are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-hardware")
        .description("Uses browser hardware permissions")
        .category(Category::new("browser/permissions").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("navigator.hid.requestDevice")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("navigator.serial.requestPort")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("navigator.usb.requestDevice")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
