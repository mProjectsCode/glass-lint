//! Obsidian plugin load/unload rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects plugin-manager and returned-plugin load/unload operations.
pub fn rule() -> Rule {
    Rule::builder("plugins.load-unload")
        .label("Loads or unloads plugins at runtime")
        .category("plugins")
        .severity(Severity::Warning)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.plugins.loadPlugin"))
        .matcher(Matcher::rooted_member_call("app.plugins.unloadPlugin"))
        .matcher(Matcher::returned_member_call(
            "app.plugins.getPlugin",
            "load",
        ))
        .matcher(Matcher::returned_member_call(
            "app.plugins.getPlugin",
            "unload",
        ))
        .matcher(Matcher::returned_member_call("app.plugins.plugins", "load"))
        .matcher(Matcher::returned_member_call(
            "app.plugins.plugins",
            "unload",
        ))
        .build()
        .unwrap()
}
