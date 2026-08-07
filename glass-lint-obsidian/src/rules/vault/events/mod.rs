//! Obsidian vault-event registration rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity, ValueMatcher};

/// Detects rooted registration through `app.vault.on`, including `this.app`,
/// direct receiver aliases, and static computed properties. Source-ordered
/// reassignment and lexical shadowing are respected. Argument zero must be a
/// static string in the public vault-event set (`create`, `delete`, `modify`,
/// `rename`); handler identity, remaining arguments, and other
/// event methods are ignored.
pub fn rule() -> Rule {
    Rule::builder("vault.events")
        .description("Registers vault events")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(
            EventQuery::member_call_rooted("app.vault.on")
                .map(|q| {
                    q.with_arg(
                        0,
                        ValueMatcher::static_string()
                            .equals_any(["create", "delete", "modify", "rename"])
                            .unwrap(),
                    )
                    .unwrap()
                    .into_query()
                })
                .unwrap(),
        )
        .build()
        .unwrap()
}
