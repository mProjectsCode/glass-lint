//! Obsidian cached-frontmatter rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects rooted reads of `app.metadataCache.getFileCache.frontmatter`,
/// including aliases and static computed properties. It does not infer
/// frontmatter from arbitrary objects, does not follow shadowed or reassigned
/// aliases, and does not analyze the cached value itself.
pub fn rule() -> Rule {
    Rule::builder("metadata.frontmatter-read")
        .description("Reads cached frontmatter")
        .category("metadata")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(MatcherDecl::rooted_member_read(
            "app.metadataCache.getFileCache.frontmatter",
        ))
        .declaration(MatcherDecl::returned_member_read(
            "app.metadataCache.getFileCache",
            "frontmatter",
        ))
        .declaration(MatcherDecl::module_member_call(
            "obsidian",
            "parseFrontMatterAliases",
        ))
        .declaration(MatcherDecl::module_member_call(
            "obsidian",
            "parseFrontMatterTags",
        ))
        .build()
        .unwrap()
}
