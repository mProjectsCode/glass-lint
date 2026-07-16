//! Obsidian platform-branching rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects reads of the seven exact `obsidian.Platform` flags configured by
/// this rule. Module namespace aliases, optional chains, and static computed
/// properties retain module provenance; local lookalikes, shadowed namespaces,
/// dynamic properties, and unlisted flags are excluded.
pub fn rule() -> Rule {
    Rule::builder("platform.branching")
        .label("Checks Obsidian platform flags")
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
        .build()
        .unwrap()
}
