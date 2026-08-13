//! Markdown postprocessor rule definition.

use glass_lint_core::rules::{Confidence, QueryDecl, Rule, Severity};

/// Detects the syntactic member chain `this.registerMarkdownPostProcessor`.
/// The instance matcher requires a proven Obsidian `Plugin` receiver and
/// follows proven receiver and callable aliases. Static computed names are
/// accepted; reassignment, dynamic properties, and near-name methods are
/// excluded.
pub fn rule() -> Rule {
    Rule::catalog_builder("markdown.postprocessor")
        .description("Registers markdown postprocessors")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance(
            "obsidian",
            "Plugin",
            "registerMarkdownPostProcessor",
        ))
        .build()
        .unwrap()
}
