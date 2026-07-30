//! Obsidian platform-branching rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

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
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isMobile",
        ))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isDesktop",
        ))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isIosApp",
        ))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isAndroidApp",
        ))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isMacOS",
        ))
        .query(EventQuery::member_read_module("obsidian", "Platform.isWin"))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isLinux",
        ))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isDesktopApp",
        ))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isMobileApp",
        ))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isPhone",
        ))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isTablet",
        ))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.isSafari",
        ))
        .query(EventQuery::member_read_module(
            "obsidian",
            "Platform.resourcePathPrefix",
        ))
        .build()
        .unwrap()
}
