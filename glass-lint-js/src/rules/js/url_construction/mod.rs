//! URL-constructor rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(MatcherDecl::global_constructor("URL"))
        .declaration(MatcherDecl::global_constructor("URLSearchParams"))
        .declaration(MatcherDecl::rooted_member_call("URL.parse"))
        .declaration(MatcherDecl::rooted_member_call("URL.canParse"))
        .declaration(MatcherDecl::rooted_member_call("URL.createObjectURL"))
        .declaration(MatcherDecl::rooted_member_call("URL.revokeObjectURL"))
        .declaration(MatcherDecl::string_contains("http://"))
        .declaration(MatcherDecl::string_contains("https://"))
        .build()
        .unwrap()
}
