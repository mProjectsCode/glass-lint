//! URL-constructor rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects construction through the unshadowed global `URL` and
/// `URLSearchParams` constructors, selected static URL methods, and static
/// HTTP(S) URL literals. Direct aliases retain global provenance until
/// reassigned, while local shadows and lookalikes are excluded. Constructor
/// arguments are intentionally not inspected.
pub fn rule() -> Rule {
    Rule::builder("network.url-construction")
        .description("Constructs or references URLs")
        .category(Category::new("language/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(EventQuery::constructor_global("URL"))
        .query(EventQuery::constructor_global("URLSearchParams"))
        .query(EventQuery::member_call_rooted("URL.parse"))
        .query(EventQuery::member_call_rooted("URL.canParse"))
        .query(EventQuery::member_call_rooted("URL.createObjectURL"))
        .query(EventQuery::member_call_rooted("URL.revokeObjectURL"))
        .query(EventQuery::string_contains("http://"))
        .query(EventQuery::string_contains("https://"))
        .build()
        .unwrap()
}
