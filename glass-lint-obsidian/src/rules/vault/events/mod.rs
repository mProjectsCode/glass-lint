//! Obsidian vault-event registration rule definition.

use glass_lint_core::rules::{Confidence, MemberCallMatcher, Rule, Severity, ValueMatcher};

/// Detects rooted registration through `app.vault.on`, including `this.app`,
/// direct receiver aliases, and static computed properties. Source-ordered
/// reassignment and lexical shadowing are respected. Argument zero must be a
/// static string in the supported vault-event set (`changed`, `created`,
/// `create`, `deleted`, `delete`, `modified`, `modify`, `renamed`, or
/// `rename`); handler identity, remaining arguments, and other event methods
/// are ignored.
pub fn rule() -> Rule {
    Rule::builder("vault.events")
        .description("Registers vault events")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(MemberCallMatcher::rooted("app.vault.on").arg(
            0,
            ValueMatcher::static_string().equals_any([
                "changed", "created", "create", "deleted", "delete", "modified", "modify",
                "renamed", "rename",
            ]),
        ))
        .build()
        .unwrap()
}
