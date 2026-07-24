//! URL-constructor rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(
            MatcherDecl::builder()
                .constructor_global("URL")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .constructor_global("URLSearchParams")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("URL.parse")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("URL.canParse")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("URL.createObjectURL")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("URL.revokeObjectURL")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("http://")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("https://")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
