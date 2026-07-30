//! Obsidian workspace-leaf management rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

/// Detects rooted calls to workspace leaf creation, lookup, traversal, and
/// management methods on `app.workspace`. Provenance follows `this.app`,
/// workspace aliases, static computed properties, source-ordered alias
/// reassignment, and lexical shadowing. Dynamic or unlisted members, local
/// lookalikes, and call arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("workspace.leaf-management")
        .description("Manages workspace leaves")
        .category(Category::new("workspace").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(EventQuery::member_call_rooted(
            "app.workspace.getLeavesOfType",
        ))
        .query(EventQuery::member_call_rooted(
            "app.workspace.detachLeavesOfType",
        ))
        .query(EventQuery::member_call_rooted("app.workspace.revealLeaf"))
        .query(EventQuery::member_call_rooted("app.workspace.getLeaf"))
        .query(EventQuery::member_call_rooted("app.workspace.getLeafById"))
        .query(EventQuery::member_call_rooted("app.workspace.getLeftLeaf"))
        .query(EventQuery::member_call_rooted("app.workspace.getRightLeaf"))
        .query(EventQuery::member_call_rooted(
            "app.workspace.ensureSideLeaf",
        ))
        .query(EventQuery::member_call_rooted(
            "app.workspace.iterateRootLeaves",
        ))
        .query(EventQuery::member_call_rooted(
            "app.workspace.iterateAllLeaves",
        ))
        .query(EventQuery::member_call_rooted(
            "app.workspace.setActiveLeaf",
        ))
        .query(EventQuery::member_call_rooted(
            "app.workspace.moveLeafToPopout",
        ))
        .query(EventQuery::member_call_rooted(
            "app.workspace.openPopoutLeaf",
        ))
        .build()
        .unwrap()
}
