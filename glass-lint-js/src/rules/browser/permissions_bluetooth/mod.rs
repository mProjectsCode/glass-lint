//! Browser Bluetooth permission rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects calls to unshadowed `navigator.bluetooth.requestDevice`, including
/// calls through aliases of `navigator.bluetooth`. Local lookalikes and
/// reassigned aliases are excluded by rooted provenance tracking.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-bluetooth")
        .description("Uses browser Bluetooth")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call(
            "navigator.bluetooth.requestDevice",
        ))
        .build()
        .unwrap()
}
