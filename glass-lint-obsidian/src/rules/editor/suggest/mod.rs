//! Obsidian editor-suggestion registration rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects the syntactic member chain `this.registerEditorSuggest`.
/// The instance matcher requires a proven Obsidian `Plugin` receiver and
/// accepts static computed names resolving to the configured method; dynamic
/// properties, aliases, reassignment, and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("editor.suggest")
        .description("Registers editor suggestions")
        .category(Category::new("editor").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "registerEditorSuggest")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
