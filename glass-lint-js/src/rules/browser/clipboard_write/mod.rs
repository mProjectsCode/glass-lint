//! Browser clipboard-write rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects calls to the unshadowed browser clipboard write APIs, including
/// aliases derived from those APIs. Shadowed `navigator` bindings and aliases
/// that have been reassigned are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.clipboard-write")
        .description("Writes clipboard data")
        .category(Category::new("browser/clipboard").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted("navigator.clipboard.write"))
        .query(EventQuery::member_call_rooted(
            "navigator.clipboard.writeText",
        ))
        .query(
            EventQuery::member_call_rooted("document.execCommand")
                .map(|q| {
                    q.with_arg_static_strings(0, ["copy", "cut"])
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .build()
        .unwrap()
}
