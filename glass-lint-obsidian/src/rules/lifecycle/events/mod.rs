//! Obsidian lifecycle-registration rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects the syntactic lifecycle-registration chains
/// `this.registerEvent`, `this.registerDomEvent`, `this.registerInterval`, and
/// `this.registerObsidianProtocolHandler`. Bases and CLI registration have
/// dedicated provider rules.
/// The instance matcher requires a proven Obsidian `Plugin` receiver and
/// accepts static computed names; aliases, reassignment, dynamic properties,
/// and near-name methods are excluded.
pub fn rule() -> Rule {
    Rule::builder("lifecycle.events")
        .description("Registers Obsidian lifecycle events")
        .category(Category::new("lifecycle").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "registerEvent")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "registerDomEvent")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "registerInterval")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_instance("obsidian", "Plugin", "registerObsidianProtocolHandler")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
