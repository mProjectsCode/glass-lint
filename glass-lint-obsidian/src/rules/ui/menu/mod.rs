//! Obsidian menu rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects proven `obsidian.Menu` instance calls. Unproven callback parameters,
/// aliases, and same-shaped local receivers are excluded.
pub fn rule() -> Rule {
    Rule::builder("ui.menu")
        .description("Uses Obsidian menus")
        .category(Category::new("ui").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Menu", "addItem")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
