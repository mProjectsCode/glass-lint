//! Obsidian plugin load/unload rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects plugin-manager and returned-plugin load/unload operations.
pub fn rule() -> Rule {
    Rule::builder("plugins.load-unload")
        .description("Loads or unloads plugins at runtime")
        .category("plugins")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::rooted_member_call("app.plugins.loadPlugin"))
        .declaration(MatcherDecl::rooted_member_call("app.plugins.unloadPlugin"))
        .declaration(MatcherDecl::returned_member_call(
            "app.plugins.getPlugin",
            "load",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "app.plugins.getPlugin",
            "unload",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "app.plugins.plugins",
            "load",
        ))
        .declaration(MatcherDecl::returned_member_call(
            "app.plugins.plugins",
            "unload",
        ))
        .build()
        .unwrap()
}
