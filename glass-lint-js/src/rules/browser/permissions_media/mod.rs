//! Browser media-capture permission rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects unshadowed `navigator.mediaDevices.getUserMedia` and
/// `getDisplayMedia` calls and aliases
/// derived from that browser API. Locally shadowed `navigator` bindings and
/// aliases that are later reassigned do not retain browser provenance.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-media")
        .description("Uses browser media capture")
        .category(Category::new("browser/permissions").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted(
            "navigator.mediaDevices.getUserMedia",
        ))
        .query(EventQuery::member_call_rooted(
            "navigator.mediaDevices.getDisplayMedia",
        ))
        .query(EventQuery::member_call_rooted(
            "navigator.mediaDevices.enumerateDevices",
        ))
        .build()
        .unwrap()
}
