//! Browser media-capture permission rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects unshadowed `navigator.mediaDevices.getUserMedia` and
/// `getDisplayMedia` calls and aliases
/// derived from that browser API. Locally shadowed `navigator` bindings and
/// aliases that are later reassigned do not retain browser provenance.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-media")
        .description("Uses browser media capture")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call(
            "navigator.mediaDevices.getUserMedia",
        ))
        .matcher(Matcher::rooted_member_call(
            "navigator.mediaDevices.getDisplayMedia",
        ))
        .matcher(Matcher::rooted_member_call(
            "navigator.mediaDevices.enumerateDevices",
        ))
        .matcher(Matcher::rooted_member_call(
            "window.navigator.mediaDevices.getUserMedia",
        ))
        .matcher(Matcher::rooted_member_call(
            "window.navigator.mediaDevices.getDisplayMedia",
        ))
        .matcher(Matcher::rooted_member_call(
            "window.navigator.mediaDevices.enumerateDevices",
        ))
        .matcher(Matcher::rooted_member_call(
            "self.navigator.mediaDevices.getUserMedia",
        ))
        .matcher(Matcher::rooted_member_call(
            "self.navigator.mediaDevices.getDisplayMedia",
        ))
        .matcher(Matcher::rooted_member_call(
            "self.navigator.mediaDevices.enumerateDevices",
        ))
        .matcher(Matcher::rooted_member_call(
            "globalThis.navigator.mediaDevices.getUserMedia",
        ))
        .matcher(Matcher::rooted_member_call(
            "globalThis.navigator.mediaDevices.getDisplayMedia",
        ))
        .matcher(Matcher::rooted_member_call(
            "globalThis.navigator.mediaDevices.enumerateDevices",
        ))
        .build()
        .unwrap()
}
