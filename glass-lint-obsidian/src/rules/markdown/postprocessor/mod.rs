//! Markdown postprocessor rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects the syntactic member chain `this.registerMarkdownPostProcessor`.
/// The instance matcher requires a proven Obsidian `Plugin` receiver and does
/// not follow aliases or reassignment. Static computed names are accepted;
/// dynamic properties and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("markdown.postprocessor")
        .description("Registers markdown postprocessors")
        .category(Category::new("markdown").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "registerMarkdownPostProcessor")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
