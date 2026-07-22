//! Obsidian platform-branching rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects reads of the configured `obsidian.Platform` flags and resource path
/// prefix. Module namespace aliases, optional chains, and static computed
/// properties retain module provenance; local lookalikes, shadowed namespaces,
/// dynamic properties, and unlisted flags are excluded.
pub fn rule() -> Rule {
    Rule::builder("platform.branching")
        .description("Checks Obsidian platform flags")
        .category("platform")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isMobile",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isDesktop",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isIosApp",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isAndroidApp",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isMacOS",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isWin",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isLinux",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isDesktopApp",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isMobileApp",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isPhone",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isTablet",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.isSafari",
        ))
        .declaration(MatcherDecl::module_member_read(
            "obsidian",
            "Platform.resourcePathPrefix",
        ))
        .build()
        .unwrap()
}
