use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects reads from Obsidian's plugin manager: instances, manifests, and
/// enabled-plugin state. The rooted collection read intentionally also covers
/// static and dynamic plugin keys without guessing the key's value.
pub fn rule() -> Rule {
    Rule::builder("plugins.access")
        .label("Accesses other plugins")
        .category("plugins")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::rooted_member_call("app.plugins.getPlugin"))
        .matcher(Matcher::rooted_member_read("app.plugins.plugins"))
        .matcher(Matcher::rooted_member_read("app.plugins.manifests"))
        .matcher(Matcher::rooted_member_read("app.plugins.enabledPlugins"))
        .build()
        .unwrap()
}
