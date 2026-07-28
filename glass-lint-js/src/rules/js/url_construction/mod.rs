//! URL-constructor rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

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
        .query(QueryDecl::constructor_global("URL"))
        .query(QueryDecl::constructor_global("URLSearchParams"))
        .query(QueryDecl::member_call_rooted("URL.parse"))
        .query(QueryDecl::member_call_rooted("URL.canParse"))
        .query(QueryDecl::member_call_rooted("URL.createObjectURL"))
        .query(QueryDecl::member_call_rooted("URL.revokeObjectURL"))
        .query(QueryDecl::string_contains("http://"))
        .query(QueryDecl::string_contains("https://"))
        .build()
        .unwrap()
}
