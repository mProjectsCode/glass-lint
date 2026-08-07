//! Markdown link-helper rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

/// Detects calls to the exact `parseLinktext`, `normalizePath`, and
/// `getLinkpath` exports of the `obsidian` module. ESM/CommonJS aliases retain
/// module provenance, while similar modules, shadowed loaders, and reassigned
/// aliases are excluded; arguments and later helper behavior are not analyzed.
pub fn rule() -> Rule {
    Rule::catalog_builder("markdown.link")
        .description("Uses markdown link helpers")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(EventQuery::member_call_module("obsidian", "parseLinktext"))
        .query(EventQuery::member_call_module("obsidian", "normalizePath"))
        .query(EventQuery::member_call_module("obsidian", "getLinkpath"))
        .query(EventQuery::member_call_rooted(
            "app.metadataCache.fileToLinktext",
        ))
        .query(EventQuery::member_call_rooted(
            "app.fileManager.generateMarkdownLink",
        ))
        .query(EventQuery::member_call_module("obsidian", "resolveSubpath"))
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
