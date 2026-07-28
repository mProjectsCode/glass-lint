//! Obsidian menu rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects proven `obsidian.Menu` instance calls. Unproven callback parameters,
/// aliases, and same-shaped local receivers are excluded.
pub fn rule() -> Rule {
    Rule::builder("ui.menu")
        .description("Uses Obsidian menus")
        .category(Category::new("ui").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::member_call_instance("obsidian", "Menu", "addItem"))
        .build()
        .unwrap()
}
