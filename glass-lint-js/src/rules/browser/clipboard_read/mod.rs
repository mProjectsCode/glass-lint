//! Browser clipboard-read rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects calls to the unshadowed browser clipboard read APIs, including
/// aliases derived from those APIs. Shadowed `navigator` bindings and aliases
/// that have been reassigned are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.clipboard-read")
        .description("Reads clipboard data")
        .category(Category::new("browser/clipboard").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("navigator.clipboard.read")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("navigator.clipboard.readText")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("document.execCommand")
                .arg_static_strings(0, ["paste"])
                .build()
                .unwrap(),
        )
        .build()
        .unwrap()
}
