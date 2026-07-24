//! Obsidian metadata-cache access rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted reads of `app.metadataCache`, `resolvedLinks`, and
/// `unresolvedLinks`, plus calls to the three configured cache lookup methods.
/// Rooted aliases and static computed properties retain provenance. The broad
/// `app.metadataCache` read may still report when a later member is dynamic or
/// unlisted; shadowed or reassigned aliases are excluded, and call arguments
/// are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("metadata.cache-read")
        .description("Reads Obsidian metadata cache")
        .category(Category::new("metadata").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("app.metadataCache")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("app.metadataCache.resolvedLinks")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_rooted("app.metadataCache.unresolvedLinks")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.metadataCache.getFileCache")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.metadataCache.getCache")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.metadataCache.getFirstLinkpathDest")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
