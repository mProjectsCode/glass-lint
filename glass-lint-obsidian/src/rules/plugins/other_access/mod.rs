use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};
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
