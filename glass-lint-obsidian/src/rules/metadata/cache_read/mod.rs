//! Obsidian metadata-cache access rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects rooted reads of `app.metadataCache`, `resolvedLinks`, and
/// `unresolvedLinks`, plus calls to the three configured cache lookup methods.
/// Rooted aliases and static computed properties retain provenance. The broad
/// `app.metadataCache` read may still report when a later member is dynamic or
/// unlisted; shadowed or reassigned aliases are excluded, and call arguments
/// are not analyzed.
pub fn rule() -> Rule {
    Rule::catalog_builder("metadata.cache-read")
        .description("Reads Obsidian metadata cache")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_read_rooted("app.metadataCache"))
        .query(EventQuery::member_read_rooted(
            "app.metadataCache.resolvedLinks",
        ))
        .query(EventQuery::member_read_rooted(
            "app.metadataCache.unresolvedLinks",
        ))
        .query(EventQuery::member_call_rooted(
            "app.metadataCache.getFileCache",
        ))
        .query(EventQuery::member_call_rooted("app.metadataCache.getCache"))
        .query(EventQuery::member_call_rooted(
            "app.metadataCache.getFirstLinkpathDest",
        ))
        .build()
        .unwrap()
}
