//! Obsidian workspace-leaf management rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted calls to workspace leaf creation, lookup, traversal, and
/// management methods on `app.workspace`. Provenance follows `this.app`,
/// workspace aliases, static computed properties, source-ordered alias
/// reassignment, and lexical shadowing. Dynamic or unlisted members, local
/// lookalikes, and call arguments are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("workspace.leaf-management")
        .description("Manages workspace leaves")
        .category("workspace")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.workspace.getLeavesOfType"))
        .matcher(Matcher::rooted_member_call(
            "app.workspace.detachLeavesOfType",
        ))
        .matcher(Matcher::rooted_member_call("app.workspace.revealLeaf"))
        .matcher(Matcher::rooted_member_call("app.workspace.getLeaf"))
        .matcher(Matcher::rooted_member_call("app.workspace.getLeafById"))
        .matcher(Matcher::rooted_member_call("app.workspace.getLeftLeaf"))
        .matcher(Matcher::rooted_member_call("app.workspace.getRightLeaf"))
        .matcher(Matcher::rooted_member_call("app.workspace.ensureSideLeaf"))
        .matcher(Matcher::rooted_member_call(
            "app.workspace.iterateRootLeaves",
        ))
        .matcher(Matcher::rooted_member_call(
            "app.workspace.iterateAllLeaves",
        ))
        .matcher(Matcher::rooted_member_call("app.workspace.setActiveLeaf"))
        .matcher(Matcher::rooted_member_call(
            "app.workspace.moveLeafToPopout",
        ))
        .matcher(Matcher::rooted_member_call("app.workspace.openPopoutLeaf"))
        .build()
        .unwrap()
}
