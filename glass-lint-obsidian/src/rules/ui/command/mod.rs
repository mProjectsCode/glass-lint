//! Obsidian command-registration rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects `this.addCommand()` registrations, including static computed
/// properties and bounded extracted aliases. The instance matcher requires a
/// proven Obsidian `Plugin` receiver; shadowing, reassignment, and dynamic
/// properties are excluded.
pub fn rule() -> Rule {
    Rule::builder("ui.command")
        .description("Registers commands")
        .category("ui")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "addCommand")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
