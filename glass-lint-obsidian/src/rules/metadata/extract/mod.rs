//! Obsidian metadata collection-extraction rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

const METADATA_FIELDS: &[&str] = &[
    "tags",
    "links",
    "embeds",
    "blocks",
    "headings",
    "sections",
    "listItems",
    "footnotes",
    "footnoteRefs",
    "referenceLinks",
    "frontmatterLinks",
    "frontmatter",
    "frontmatterAliases",
    "frontmatterTags",
    "frontmatterPosition",
];

/// Detects rooted reads of the configured `getFileCache` metadata collections:
/// `tags`, `links`, `embeds`, `blocks`, `headings`, `sections`, `listItems`,
/// `footnotes`, `footnoteRefs`, `referenceLinks`, `frontmatterLinks`,
/// `frontmatter`, `frontmatterAliases`, `frontmatterTags`, and
/// `frontmatterPosition`.
/// Rooted aliases and static computed properties retain provenance, while
/// shadowed or reassigned aliases, dynamic properties, and unlisted collections
/// are excluded. The rule reads member chains; it does not infer collections
/// from arbitrary `getFileCache(...)` return values.
pub fn rule() -> Rule {
    let mut builder = Rule::builder("metadata.extract")
        .description("Extracts tags, links, embeds, blocks, or headings")
        .category("metadata")
        .severity(Severity::Info)
        .confidence(Confidence::Medium);

    for field in METADATA_FIELDS {
        builder = builder.declaration(
            MatcherDecl::builder()
                .member_read_rooted(format!("app.metadataCache.getFileCache.{field}"))
                .build()
                .expect("valid matcher declaration"),
        );
    }
    for field in METADATA_FIELDS {
        builder = builder.declaration(
            MatcherDecl::builder()
                .member_read_returned("app.metadataCache.getFileCache", *field)
                .build()
                .expect("valid matcher declaration"),
        );
    }

    builder.build().unwrap()
}
