//! Markdown link-helper rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects calls to the exact `parseLinktext`, `normalizePath`, and
/// `getLinkpath` exports of the `obsidian` module. ESM/CommonJS aliases retain
/// module provenance, while similar modules, shadowed loaders, and reassigned
/// aliases are excluded; arguments and later helper behavior are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("markdown.link")
        .description("Uses markdown link helpers")
        .category("markdown")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "parseLinktext")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "normalizePath")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "getLinkpath")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "fileToLinktext")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "generateMarkdownLink")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "resolveSubpath")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "parseSubpath")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "parseFrontMatterAliases")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_module("obsidian", "parseFrontMatterTags")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
