//! Browser clipboard-read rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects calls to the unshadowed browser clipboard read APIs, including
/// aliases derived from those APIs. Shadowed `navigator` bindings and aliases
/// that have been reassigned are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.clipboard-read")
        .description("Reads clipboard data")
        .category(Category::new("browser/clipboard").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted("navigator.clipboard.read"))
        .query(EventQuery::member_call_rooted(
            "navigator.clipboard.readText",
        ))
        .query(
            EventQuery::member_call_rooted("document.execCommand")
                .map(|q| {
                    q.with_arg_static_strings(0, ["paste"])
                        .unwrap()
                        .into_query()
                })
                .unwrap(),
        )
        .build()
        .unwrap()
}
