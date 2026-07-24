//! Obsidian platform-branching rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

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
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isMobile")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isDesktop")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isIosApp")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isAndroidApp")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isMacOS")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isWin")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isLinux")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isDesktopApp")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isMobileApp")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isPhone")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isTablet")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.isSafari")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_read_module("obsidian", "Platform.resourcePathPrefix")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
