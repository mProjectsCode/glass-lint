//! Markdown link-helper rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects calls to the exact `parseLinktext`, `normalizePath`, and
/// `getLinkpath` exports of the `obsidian` module. ESM/CommonJS aliases retain
/// module provenance, while similar modules, shadowed loaders, and reassigned
/// aliases are excluded; arguments and later helper behavior are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("markdown.link")
        .description("Uses markdown link helpers")
        .category(Category::new("markdown").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::member_call_module("obsidian", "parseLinktext"))
        .query(QueryDecl::member_call_module("obsidian", "normalizePath"))
        .query(QueryDecl::member_call_module("obsidian", "getLinkpath"))
        .query(QueryDecl::member_call_module("obsidian", "fileToLinktext"))
        .query(QueryDecl::member_call_module(
            "obsidian",
            "generateMarkdownLink",
        ))
        .query(QueryDecl::member_call_module("obsidian", "resolveSubpath"))
        .query(QueryDecl::member_call_module("obsidian", "parseSubpath"))
        .query(QueryDecl::member_call_module(
            "obsidian",
            "parseFrontMatterAliases",
        ))
        .query(QueryDecl::member_call_module(
            "obsidian",
            "parseFrontMatterTags",
        ))
        .build()
        .unwrap()
}
