//! Browser Bluetooth permission rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects calls to unshadowed `navigator.bluetooth.requestDevice`, including
/// calls through aliases of `navigator.bluetooth`. Local lookalikes and
/// reassigned aliases are excluded by rooted provenance tracking.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-bluetooth")
        .description("Uses browser Bluetooth")
        .category(Category::new("browser/permissions").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("navigator.bluetooth.requestDevice")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
