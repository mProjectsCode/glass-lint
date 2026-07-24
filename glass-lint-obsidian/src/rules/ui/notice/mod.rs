//! Obsidian notice rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects the exact global `Notice` constructor plus constructors and
/// subclasses proven to come from the `obsidian` module. Local/shadowed and
/// reassigned names are excluded, while global-object, ESM, namespace, and
/// CommonJS provenance is followed. Constructor arguments and subclass bodies
/// are not analyzed.
pub fn rule() -> Rule {
    Rule::builder("ui.notice")
        .description("Uses Obsidian notices")
        .category(Category::new("ui").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .constructor_global("Notice")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .constructor_module("obsidian", "Notice")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .class_module("obsidian", "Notice")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
