//! Obsidian workspace-leaf management rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

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
        .query(QueryDecl::member_call_rooted("app.workspace.getLeavesOfType"))
        .query(QueryDecl::member_call_rooted("app.workspace.detachLeavesOfType"))
        .query(QueryDecl::member_call_rooted("app.workspace.revealLeaf"))
        .query(QueryDecl::member_call_rooted("app.workspace.getLeaf"))
        .query(QueryDecl::member_call_rooted("app.workspace.getLeafById"))
        .query(QueryDecl::member_call_rooted("app.workspace.getLeftLeaf"))
        .query(QueryDecl::member_call_rooted("app.workspace.getRightLeaf"))
        .query(QueryDecl::member_call_rooted("app.workspace.ensureSideLeaf"))
        .query(QueryDecl::member_call_rooted("app.workspace.iterateRootLeaves"))
        .query(QueryDecl::member_call_rooted("app.workspace.iterateAllLeaves"))
        .query(QueryDecl::member_call_rooted("app.workspace.setActiveLeaf"))
        .query(QueryDecl::member_call_rooted("app.workspace.moveLeafToPopout"))
        .query(QueryDecl::member_call_rooted("app.workspace.openPopoutLeaf"))
        .build()
        .unwrap()
}
