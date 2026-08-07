//! Obsidian cached-frontmatter rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, QueryDecl, Rule, Severity};

/// Detects rooted reads of `app.metadataCache.getFileCache.frontmatter`,
/// including aliases and static computed properties. It does not infer
/// frontmatter from arbitrary objects, does not follow shadowed or reassigned
/// aliases, and does not analyze the cached value itself.
pub fn rule() -> Rule {
    Rule::catalog_builder("metadata.frontmatter-read")
        .description("Reads cached frontmatter")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(EventQuery::member_read_rooted(
            "app.metadataCache.getFileCache.frontmatter",
        ))
        .query(QueryDecl::member_read_returned(
            "app.metadataCache.getFileCache",
            "frontmatter",
        ))
        .query(QueryDecl::member_read_returned(
            "app.metadataCache.getCache",
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
        .query(EventQuery::member_call_module(
            "obsidian",
            "parseFrontMatterEntry",
        ))
        .query(EventQuery::member_call_module(
            "obsidian",
            "parseFrontMatterStringArray",
        ))
        .build()
        .unwrap()
}
