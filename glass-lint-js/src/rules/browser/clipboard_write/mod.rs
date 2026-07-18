//! Browser clipboard-write rule definition.

use glass_lint_core::rules::{Confidence, Matcher, MemberCallMatcher, Rule, Severity};

/// Detects calls to the unshadowed browser clipboard write APIs, including
/// aliases derived from those APIs. Shadowed `navigator` bindings and aliases
/// that have been reassigned are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.clipboard-write")
        .description("Writes clipboard data")
        .category("browser/clipboard")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("navigator.clipboard.write"))
        .matcher(Matcher::rooted_member_call("navigator.clipboard.writeText"))
        .matcher(Matcher::rooted_member_call(
            "window.navigator.clipboard.write",
        ))
        .matcher(Matcher::rooted_member_call(
            "window.navigator.clipboard.writeText",
        ))
        .matcher(Matcher::rooted_member_call(
            "self.navigator.clipboard.write",
        ))
        .matcher(Matcher::rooted_member_call(
            "self.navigator.clipboard.writeText",
        ))
        .matcher(Matcher::rooted_member_call(
            "globalThis.navigator.clipboard.write",
        ))
        .matcher(Matcher::rooted_member_call(
            "globalThis.navigator.clipboard.writeText",
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("document.execCommand")
                .arg_static_strings(0, ["copy", "cut"]),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("window.document.execCommand")
                .arg_static_strings(0, ["copy", "cut"]),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("globalThis.document.execCommand")
                .arg_static_strings(0, ["copy", "cut"]),
        ))
        .build()
        .unwrap()
}
