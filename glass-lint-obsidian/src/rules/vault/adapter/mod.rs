//! Obsidian vault-adapter access rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects operations on the rooted `DataAdapter` exposed by
/// `app.vault.adapter`. Reading the adapter object alone is not a filesystem
/// operation. Direct rooted aliases and static computed properties are
/// supported; arbitrary wrappers and dynamic methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("vault.adapter")
        .description("Uses adapter-level vault filesystem APIs")
        .category(Category::new("vault").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted("app.vault.adapter.exists"))
        .query(EventQuery::member_call_rooted("app.vault.adapter.stat"))
        .query(EventQuery::member_call_rooted("app.vault.adapter.list"))
        .query(EventQuery::member_call_rooted("app.vault.adapter.read"))
        .query(EventQuery::member_call_rooted("app.vault.adapter.write"))
        .query(EventQuery::member_call_rooted("app.vault.adapter.append"))
        .query(EventQuery::member_call_rooted("app.vault.adapter.process"))
        .query(EventQuery::member_call_rooted("app.vault.adapter.mkdir"))
        .query(EventQuery::member_call_rooted("app.vault.adapter.remove"))
        .query(EventQuery::member_call_rooted("app.vault.adapter.rename"))
        .query(EventQuery::member_call_rooted("app.vault.adapter.copy"))
        .query(EventQuery::member_call_rooted(
            "app.vault.adapter.getFullPath",
        ))
        .build()
        .unwrap()
}
