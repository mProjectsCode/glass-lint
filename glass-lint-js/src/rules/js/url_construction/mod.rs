//! URL-constructor rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects construction through the unshadowed global `URL` and
/// `URLSearchParams` constructors, selected static URL methods, and static
/// HTTP(S) URL literals. Direct aliases retain global provenance until
/// reassigned, while local shadows and lookalikes are excluded. Constructor
/// arguments are intentionally not inspected.
pub fn rule() -> Rule {
    Rule::builder("network.url-construction")
        .description("Constructs or references URLs")
        .category("language/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::global_constructor("URL"))
        .matcher(Matcher::global_constructor("URLSearchParams"))
        .matcher(Matcher::rooted_member_call("URL.parse"))
        .matcher(Matcher::rooted_member_call("URL.canParse"))
        .matcher(Matcher::rooted_member_call("URL.createObjectURL"))
        .matcher(Matcher::rooted_member_call("URL.revokeObjectURL"))
        .matcher(Matcher::string_contains("http://"))
        .matcher(Matcher::string_contains("https://"))
        .build()
        .unwrap()
}
