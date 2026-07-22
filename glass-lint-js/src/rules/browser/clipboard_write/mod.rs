//! Browser clipboard-write rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects calls to the unshadowed browser clipboard write APIs, including
/// aliases derived from those APIs. Shadowed `navigator` bindings and aliases
/// that have been reassigned are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.clipboard-write")
        .description("Writes clipboard data")
        .category("browser/clipboard")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("navigator.clipboard.write")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("navigator.clipboard.writeText")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("document.execCommand")
                .arg_static_strings(0, ["copy", "cut"])
                .build()
                .unwrap(),
        )
        .build()
        .unwrap()
}
