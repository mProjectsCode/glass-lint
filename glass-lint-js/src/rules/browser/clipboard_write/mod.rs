//! Browser clipboard-write rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects calls to the unshadowed browser clipboard write APIs, including
/// aliases derived from those APIs. Shadowed `navigator` bindings and aliases
/// that have been reassigned are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.clipboard-write")
        .description("Writes clipboard data")
        .category(Category::new("browser/clipboard").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_rooted("navigator.clipboard.write"))
        .query(QueryDecl::member_call_rooted(
            "navigator.clipboard.writeText",
        ))
        .query(
            QueryDecl::member_call_rooted("document.execCommand")
                .with_arg_static_strings(0, ["copy", "cut"]),
        )
        .build()
        .unwrap()
}
