use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted reads of `app.metadataCache`, `resolvedLinks`, and
/// `unresolvedLinks`, plus calls to the three configured cache lookup methods.
/// Rooted aliases and static computed properties retain provenance. The broad
/// `app.metadataCache` read may still report when a later member is dynamic or
/// unlisted; shadowed or reassigned aliases are excluded, and call arguments
/// are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("metadata.cache-read")
        .label("Reads Obsidian metadata cache")
        .category("metadata")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_read("app.metadataCache"))
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.resolvedLinks",
        ))
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.unresolvedLinks",
        ))
        .matcher(Matcher::rooted_member_call(
            "app.metadataCache.getFileCache",
        ))
        .matcher(Matcher::rooted_member_call("app.metadataCache.getCache"))
        .matcher(Matcher::rooted_member_call(
            "app.metadataCache.getFirstLinkpathDest",
        ))
        .build()
        .unwrap()
}
