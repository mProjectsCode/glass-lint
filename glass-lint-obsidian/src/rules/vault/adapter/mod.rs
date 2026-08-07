//! Obsidian vault-adapter access rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

const ADAPTER_METHODS: &[&str] = &[
    "app.vault.adapter.exists",
    "app.vault.adapter.stat",
    "app.vault.adapter.list",
    "app.vault.adapter.read",
    "app.vault.adapter.write",
    "app.vault.adapter.append",
    "app.vault.adapter.process",
    "app.vault.adapter.mkdir",
    "app.vault.adapter.remove",
    "app.vault.adapter.rename",
    "app.vault.adapter.copy",
    "app.vault.adapter.getFullPath",
];

/// Detects operations on the rooted `DataAdapter` exposed by
/// `app.vault.adapter`. Reading the adapter object alone is not a filesystem
/// operation. Direct rooted aliases and static computed properties are
/// supported; arbitrary wrappers and dynamic methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("vault.adapter")
        .description("Uses adapter-level vault filesystem APIs")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .queries(
            ADAPTER_METHODS
                .iter()
                .copied()
                .map(EventQuery::member_call_rooted),
        )
        .build()
        .unwrap()
}
