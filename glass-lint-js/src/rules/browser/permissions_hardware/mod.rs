//! Browser hardware-permission rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects unshadowed WebHID, Web Serial, and WebUSB device requests. Rooted
/// aliases and static computed properties retain browser provenance; local
/// lookalikes, reassigned aliases, and dynamic properties are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-hardware")
        .description("Uses browser hardware permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted(
            "navigator.hid.requestDevice",
        ))
        .query(EventQuery::member_call_rooted(
            "navigator.serial.requestPort",
        ))
        .query(EventQuery::member_call_rooted(
            "navigator.usb.requestDevice",
        ))
        .build()
        .unwrap()
}
