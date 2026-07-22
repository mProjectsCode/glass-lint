//! Browser permission-query rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects calls to the rooted browser Permissions API. The matcher follows
/// aliases and static computed properties while rejecting shadowed or dynamic
/// receivers and property names.
pub fn rule() -> Rule {
    Rule::builder("browser.permissions-query")
        .description("Queries browser permissions")
        .category("browser/permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call(
            "navigator.permissions.query",
        ))
        .build()
        .unwrap()
}
