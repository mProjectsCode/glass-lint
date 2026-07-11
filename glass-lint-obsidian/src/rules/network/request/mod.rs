use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects calls to the exact `request` and `requestUrl` exports of the
/// `obsidian` module. ESM/CommonJS namespace and export aliases retain module
/// provenance, while similar modules, shadowed loaders or namespaces, and
/// reassigned aliases are excluded; request arguments are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("network.request")
        .label("Uses Obsidian request APIs")
        .category("network")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::module_member_call("obsidian", "request"))
        .matcher(Matcher::module_member_call("obsidian", "requestUrl"))
        .build()
        .unwrap()
}
