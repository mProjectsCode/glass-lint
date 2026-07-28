//! Browser clipboard-read rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects calls to the unshadowed browser clipboard read APIs, including
/// aliases derived from those APIs. Shadowed `navigator` bindings and aliases
/// that have been reassigned are excluded.
pub fn rule() -> Rule {
    Rule::builder("browser.clipboard-read")
        .description("Reads clipboard data")
        .category(Category::new("browser/clipboard").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_rooted("navigator.clipboard.read"))
        .query(QueryDecl::member_call_rooted("navigator.clipboard.readText"))
        .query(QueryDecl::member_call_rooted("document.execCommand")
                .with_arg_static_strings(0, ["paste"]))
        .build()
        .unwrap()
}
