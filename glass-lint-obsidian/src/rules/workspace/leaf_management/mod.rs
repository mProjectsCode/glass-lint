//! Obsidian workspace-leaf management rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.getLeavesOfType",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.detachLeavesOfType",
        ))
        .declaration(MatcherDecl::rooted_member_call("app.workspace.revealLeaf"))
        .declaration(MatcherDecl::rooted_member_call("app.workspace.getLeaf"))
        .declaration(MatcherDecl::rooted_member_call("app.workspace.getLeafById"))
        .declaration(MatcherDecl::rooted_member_call("app.workspace.getLeftLeaf"))
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.getRightLeaf",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.ensureSideLeaf",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.iterateRootLeaves",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.iterateAllLeaves",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.setActiveLeaf",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.moveLeafToPopout",
        ))
        .declaration(MatcherDecl::rooted_member_call(
            "app.workspace.openPopoutLeaf",
        ))
        .build()
        .unwrap()
}
