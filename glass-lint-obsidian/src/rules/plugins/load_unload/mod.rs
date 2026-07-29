//! Obsidian plugin load/unload rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects plugin-manager and returned-plugin load/unload operations.
pub fn rule() -> Rule {
    Rule::builder("plugins.load-unload")
        .description("Loads or unloads plugins at runtime")
        .category(Category::new("plugins").unwrap())
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .query(QueryDecl::member_call_rooted("app.plugins.loadPlugin"))
        .query(QueryDecl::member_call_rooted("app.plugins.unloadPlugin"))
        .query(QueryDecl::member_call_returned(
            "app.plugins.getPlugin",
            "load",
        ))
        .query(QueryDecl::member_call_returned(
            "app.plugins.getPlugin",
            "unload",
        ))
        .query(QueryDecl::member_call_returned(
            "app.plugins.plugins",
            "load",
        ))
        .query(QueryDecl::member_call_returned(
            "app.plugins.plugins",
            "unload",
        ))
        .build()
        .unwrap()
}
