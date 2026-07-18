//! Browser geolocation permission rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects calls to unshadowed `navigator.geolocation.getCurrentPosition` and
/// `watchPosition`,
/// including calls through aliases of `navigator.geolocation`. Local
/// lookalikes and reassigned aliases are excluded by rooted provenance
/// tracking.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-geolocation")
        .description("Uses browser geolocation")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call(
            "navigator.geolocation.getCurrentPosition",
        ))
        .matcher(Matcher::rooted_member_call(
            "navigator.geolocation.watchPosition",
        ))
        .matcher(Matcher::rooted_member_call(
            "window.navigator.geolocation.getCurrentPosition",
        ))
        .matcher(Matcher::rooted_member_call(
            "window.navigator.geolocation.watchPosition",
        ))
        .matcher(Matcher::rooted_member_call(
            "self.navigator.geolocation.getCurrentPosition",
        ))
        .matcher(Matcher::rooted_member_call(
            "self.navigator.geolocation.watchPosition",
        ))
        .matcher(Matcher::rooted_member_call(
            "globalThis.navigator.geolocation.getCurrentPosition",
        ))
        .matcher(Matcher::rooted_member_call(
            "globalThis.navigator.geolocation.watchPosition",
        ))
        .build()
        .unwrap()
}
