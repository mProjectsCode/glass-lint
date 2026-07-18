//! Browser permission-query rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects calls to the rooted browser Permissions API. The matcher follows
/// aliases and static computed properties while rejecting shadowed or dynamic
/// receivers and property names.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-query")
        .description("Queries browser permissions")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("navigator.permissions.query"))
        .matcher(Matcher::rooted_member_call(
            "window.navigator.permissions.query",
        ))
        .matcher(Matcher::rooted_member_call(
            "self.navigator.permissions.query",
        ))
        .matcher(Matcher::rooted_member_call(
            "globalThis.navigator.permissions.query",
        ))
        .build()
        .unwrap()
}
