//! Browser clipboard-read rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects calls to the unshadowed browser clipboard read APIs, including
/// aliases derived from those APIs. Shadowed `navigator` bindings and aliases
/// that have been reassigned are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.clipboard-read")
        .description("Reads clipboard data")
        .category("browser/clipboard")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call("navigator.clipboard.read"))
        .declaration(MatcherDecl::rooted_member_call(
            "navigator.clipboard.readText",
        ))
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
