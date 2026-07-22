//! Browser hardware-permission rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects unshadowed WebHID, Web Serial, and WebUSB device requests. Rooted
/// aliases and static computed properties retain browser provenance; local
/// lookalikes, reassigned aliases, and dynamic properties are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-hardware")
        .description("Uses browser hardware permissions")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call(
            "navigator.hid.requestDevice",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "navigator.serial.requestPort",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "navigator.usb.requestDevice",
        ))
        .build()
        .unwrap()
}
