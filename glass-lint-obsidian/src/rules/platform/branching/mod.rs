use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
pub(crate) fn rule() -> Rule {
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
