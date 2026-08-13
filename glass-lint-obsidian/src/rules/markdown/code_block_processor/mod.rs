//! Markdown code-block processor rule definition.

use glass_lint_core::rules::{Confidence, QueryDecl, Rule, Severity};

/// Detects the syntactic chain `this.registerMarkdownCodeBlockProcessor`.
/// The instance matcher requires a proven Obsidian `Plugin` receiver; proven
/// receiver and callable aliases retain identity through static computed names.
/// Reassignment, dynamic properties, and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::catalog_builder("markdown.code-block-processor")
        .description("Registers markdown code-block processors")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_instance(
            "obsidian",
            "Plugin",
            "registerMarkdownCodeBlockProcessor",
        ))
        .build()
        .unwrap()
}
