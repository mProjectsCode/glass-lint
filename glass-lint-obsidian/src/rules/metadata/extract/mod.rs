use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted reads of the configured `getFileCache` metadata collections:
/// `tags`, `links`, `embeds`, `blocks`, `headings`, and `sections`. Rooted
/// aliases and static computed properties retain provenance, while shadowed or
/// reassigned aliases, dynamic properties, and unlisted collections are
/// excluded. The rule reads member chains; it does not infer collections from
/// arbitrary `getFileCache(...)` return values.
pub fn rule() -> Rule {
    Rule::builder("metadata.extract")
        .label("Extracts tags, links, embeds, blocks, or headings")
        .category("metadata")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.getFileCache.tags",
        ))
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.getFileCache.links",
        ))
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.getFileCache.embeds",
        ))
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.getFileCache.blocks",
        ))
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.getFileCache.headings",
        ))
        .matcher(Matcher::rooted_member_read(
            "app.metadataCache.getFileCache.sections",
        ))
        .matcher(Matcher::returned_member_read(
            "app.metadataCache.getFileCache",
            "tags",
        ))
        .matcher(Matcher::returned_member_read(
            "app.metadataCache.getFileCache",
            "links",
        ))
        .matcher(Matcher::returned_member_read(
            "app.metadataCache.getFileCache",
            "embeds",
        ))
        .matcher(Matcher::returned_member_read(
            "app.metadataCache.getFileCache",
            "blocks",
        ))
        .matcher(Matcher::returned_member_read(
            "app.metadataCache.getFileCache",
            "headings",
        ))
        .matcher(Matcher::returned_member_read(
            "app.metadataCache.getFileCache",
            "sections",
        ))
        .build()
        .unwrap()
}
