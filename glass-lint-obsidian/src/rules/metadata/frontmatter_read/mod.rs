//! Obsidian cached-frontmatter rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, QueryDecl, Rule, Severity};

/// Detects rooted reads of `app.metadataCache.getFileCache.frontmatter`,
/// including aliases and static computed properties. It does not infer
/// frontmatter from arbitrary objects, does not follow shadowed or reassigned
/// aliases, and does not analyze the cached value itself.
pub fn rule() -> Rule {
    Rule::builder("metadata.frontmatter-read")
        .description("Reads cached frontmatter")
        .category(Category::new("metadata").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(EventQuery::member_read_rooted(
            "app.metadataCache.getFileCache.frontmatter",
        ))
        .query(QueryDecl::member_read_returned(
            "app.metadataCache.getFileCache",
            "frontmatter",
        ))
        .query(EventQuery::member_call_module(
            "obsidian",
            "parseFrontMatterAliases",
        ))
        .query(EventQuery::member_call_module(
            "obsidian",
            "parseFrontMatterTags",
        ))
        .build()
        .unwrap()
}
