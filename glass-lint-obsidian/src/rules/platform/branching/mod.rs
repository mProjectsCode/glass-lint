//! Obsidian platform-branching rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

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
        .matcher(Matcher::module_member_read("obsidian", "Platform.isMobile"))
        .matcher(Matcher::module_member_read(
            "obsidian",
            "Platform.isDesktop",
        ))
        .matcher(Matcher::module_member_read("obsidian", "Platform.isIosApp"))
        .matcher(Matcher::module_member_read(
            "obsidian",
            "Platform.isAndroidApp",
        ))
        .matcher(Matcher::module_member_read("obsidian", "Platform.isMacOS"))
        .matcher(Matcher::module_member_read("obsidian", "Platform.isWin"))
        .matcher(Matcher::module_member_read("obsidian", "Platform.isLinux"))
        .matcher(Matcher::module_member_read(
            "obsidian",
            "Platform.isDesktopApp",
        ))
        .matcher(Matcher::module_member_read(
            "obsidian",
            "Platform.isMobileApp",
        ))
        .matcher(Matcher::module_member_read("obsidian", "Platform.isPhone"))
        .matcher(Matcher::module_member_read("obsidian", "Platform.isTablet"))
        .matcher(Matcher::module_member_read("obsidian", "Platform.isSafari"))
        .matcher(Matcher::module_member_read(
            "obsidian",
            "Platform.resourcePathPrefix",
        ))
        .build()
        .unwrap()
}
