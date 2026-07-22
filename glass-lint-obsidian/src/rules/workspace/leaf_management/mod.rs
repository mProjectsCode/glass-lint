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
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.getLeavesOfType")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.detachLeavesOfType")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.revealLeaf")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.getLeaf")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.getLeafById")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.getLeftLeaf")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.getRightLeaf")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.ensureSideLeaf")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.iterateRootLeaves")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.iterateAllLeaves")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.setActiveLeaf")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.moveLeafToPopout")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("app.workspace.openPopoutLeaf")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
