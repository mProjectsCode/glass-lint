//! Browser permission-query rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects calls to the rooted browser Permissions API. The matcher follows
/// aliases and static computed properties while rejecting shadowed or dynamic
/// receivers and property names.
pub fn rule() -> Rule {
    Rule::catalog_builder("browser.permissions-query")
        .description("Queries browser permissions")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted(
            "navigator.permissions.query",
        ))
        .build()
        .unwrap()
}
