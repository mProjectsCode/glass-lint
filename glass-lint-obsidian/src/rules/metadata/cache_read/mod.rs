//! Obsidian metadata-cache access rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted reads of `app.metadataCache`, `resolvedLinks`, and
/// `unresolvedLinks`, plus calls to the three configured cache lookup methods.
/// Rooted aliases and static computed properties retain provenance. The broad
/// `app.metadataCache` read may still report when a later member is dynamic or
/// unlisted; shadowed or reassigned aliases are excluded, and call arguments
/// are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("metadata.cache-read")
        .description("Reads Obsidian metadata cache")
        .category("metadata")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_read("app.metadataCache"))
        .declaration(MatcherDecl::rooted_member_read(
            "app.metadataCache.resolvedLinks",
        ))
        .declaration(MatcherDecl::rooted_member_read(
            "app.metadataCache.unresolvedLinks",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.metadataCache.getFileCache",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.metadataCache.getCache",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.metadataCache.getFirstLinkpathDest",
        ))
        .build()
        .unwrap()
}
