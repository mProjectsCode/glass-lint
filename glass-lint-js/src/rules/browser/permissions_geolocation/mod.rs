//! Browser geolocation permission rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects calls to unshadowed `navigator.geolocation.getCurrentPosition` and
/// `watchPosition`,
/// including calls through aliases of `navigator.geolocation`. Local
/// lookalikes and reassigned aliases are excluded by rooted provenance
/// tracking.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-geolocation")
        .description("Uses browser geolocation")
        .category(Category::new("browser/permissions").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted(
            "navigator.geolocation.getCurrentPosition",
        ))
        .query(EventQuery::member_call_rooted(
            "navigator.geolocation.watchPosition",
        ))
        .build()
        .unwrap()
}
