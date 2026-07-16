//! Markdown link-helper rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects calls to the exact `parseLinktext`, `normalizePath`, and
/// `getLinkpath` exports of the `obsidian` module. ESM/CommonJS aliases retain
/// module provenance, while similar modules, shadowed loaders, and reassigned
/// aliases are excluded; arguments and later helper behavior are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("markdown.link")
        .label("Uses markdown link helpers")
        .category("markdown")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::module_member_call("obsidian", "parseLinktext"))
        .matcher(Matcher::module_member_call("obsidian", "normalizePath"))
        .matcher(Matcher::module_member_call("obsidian", "getLinkpath"))
        .build()
        .unwrap()
}
