//! Markdown link-helper rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

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
        .matcher(Matcher::module_member_call("obsidian", "parseLinktext"))
        .matcher(Matcher::module_member_call("obsidian", "normalizePath"))
        .matcher(Matcher::module_member_call("obsidian", "getLinkpath"))
        .matcher(Matcher::module_member_call("obsidian", "fileToLinktext"))
        .matcher(Matcher::module_member_call(
            "obsidian",
            "generateMarkdownLink",
        ))
        .matcher(Matcher::module_member_call("obsidian", "resolveSubpath"))
        .matcher(Matcher::module_member_call("obsidian", "parseSubpath"))
        .matcher(Matcher::module_member_call(
            "obsidian",
            "parseFrontMatterAliases",
        ))
        .matcher(Matcher::module_member_call(
            "obsidian",
            "parseFrontMatterTags",
        ))
        .build()
        .unwrap()
}
