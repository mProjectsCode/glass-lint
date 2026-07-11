use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects rooted plugin-manager calls and reads: `app.plugins.getPlugin`,
/// `app.plugins.enabledPlugins`, and `app.plugins.manifests`. Rooted aliases
/// and static computed properties retain provenance; shadowed or reassigned
/// roots, dynamic or unlisted properties, and local lookalikes do not match.
/// Arguments and the returned plugin objects are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("plugins.other-access")
        .label("Accesses other plugins")
        .category("plugins")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::rooted_member_call("app.plugins.getPlugin"))
        .matcher(Matcher::rooted_member_read("app.plugins.enabledPlugins"))
        .matcher(Matcher::rooted_member_read("app.plugins.manifests"))
        .build()
        .unwrap()
}
