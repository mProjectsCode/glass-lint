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
        .declaration(MatcherDecl::module_member_call("obsidian", "parseLinktext"))
        .declaration(MatcherDecl::module_member_call("obsidian", "normalizePath"))
        .declaration(MatcherDecl::module_member_call("obsidian", "getLinkpath"))
        .declaration(MatcherDecl::module_member_call(
            "obsidian",
            "fileToLinktext",
        ))
        .declaration(MatcherDecl::module_member_call(
            "obsidian",
            "generateMarkdownLink",
        ))
        .declaration(MatcherDecl::module_member_call(
            "obsidian",
            "resolveSubpath",
        ))
        .declaration(MatcherDecl::module_member_call("obsidian", "parseSubpath"))
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
