//! Obsidian platform-branching rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects reads of the configured `obsidian.Platform` flags and resource path
/// prefix. Module namespace aliases, optional chains, and static computed
/// properties retain module provenance; local lookalikes, shadowed namespaces,
/// dynamic properties, and unlisted flags are excluded.
pub fn rule() -> Rule {
    Rule::builder("platform.branching")
        .description("Checks Obsidian platform flags")
        .category(Category::new("platform").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .query(QueryDecl::member_read_module("obsidian", "Platform.isMobile"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isDesktop"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isIosApp"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isAndroidApp"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isMacOS"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isWin"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isLinux"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isDesktopApp"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isMobileApp"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isPhone"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isTablet"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.isSafari"))
        .query(QueryDecl::member_read_module("obsidian", "Platform.resourcePathPrefix"))
        .build()
        .unwrap()
}
