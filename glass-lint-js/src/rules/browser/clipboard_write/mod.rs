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
        .declaration(MatcherDecl::rooted_member_call("navigator.clipboard.write"))
        .declaration(MatcherDecl::rooted_member_call(
            "navigator.clipboard.writeText",
        ))
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
