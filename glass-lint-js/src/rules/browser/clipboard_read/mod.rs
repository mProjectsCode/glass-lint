//! Browser clipboard-read rule definition.

use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};

/// Detects calls to the unshadowed browser clipboard read APIs, including
/// aliases derived from those APIs. Shadowed `navigator` bindings and aliases
/// that have been reassigned are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.clipboard-read")
        .description("Reads clipboard data")
        .category("browser/clipboard")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("navigator.clipboard.read"))
        .matcher(Matcher::rooted_member_call("navigator.clipboard.readText"))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("document.execCommand").arg_static_strings(0, ["paste"]),
        ))
        .build()
        .unwrap()
}
