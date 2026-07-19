//! Browser hardware-permission rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects unshadowed WebHID, Web Serial, and WebUSB device requests. Rooted
/// aliases and static computed properties retain browser provenance; local
/// lookalikes, reassigned aliases, and dynamic properties are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-hardware")
        .description("Uses browser hardware permissions")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("navigator.hid.requestDevice"))
        .matcher(Matcher::rooted_member_call("navigator.serial.requestPort"))
        .matcher(Matcher::rooted_member_call("navigator.usb.requestDevice"))
        .build()
        .unwrap()
}
