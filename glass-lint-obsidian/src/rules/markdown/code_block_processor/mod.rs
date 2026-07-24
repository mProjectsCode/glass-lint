//! Markdown code-block processor rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects the syntactic chain `this.registerMarkdownCodeBlockProcessor`.
/// The instance matcher requires a proven Obsidian `Plugin` receiver; aliases
/// and reassignment are not followed. Static computed names are accepted while
/// dynamic properties and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("markdown.code-block-processor")
        .description("Registers markdown code-block processors")
        .category(Category::new("markdown").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "registerMarkdownCodeBlockProcessor")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
