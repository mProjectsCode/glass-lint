//! Obsidian platform-branching rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

const PLATFORM_MEMBERS: &[&str] = &[
    "Platform.isMobile",
    "Platform.isDesktop",
    "Platform.isIosApp",
    "Platform.isAndroidApp",
    "Platform.isMacOS",
    "Platform.isWin",
    "Platform.isLinux",
    "Platform.isDesktopApp",
    "Platform.isMobileApp",
    "Platform.isPhone",
    "Platform.isTablet",
    "Platform.isSafari",
    "Platform.resourcePathPrefix",
];

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
        .queries(
            PLATFORM_MEMBERS
                .iter()
                .copied()
                .map(|member| EventQuery::member_read_module("obsidian", member)),
        )
        .build()
        .unwrap()
}
