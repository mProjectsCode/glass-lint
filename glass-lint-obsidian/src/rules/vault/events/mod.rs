use glass_lint_core::rules::{
    Confidence, FlowValueMatcher, Matcher, MemberCallMatcher, Rule, Severity,
};

/// Detects rooted registration through `app.vault.on`, including `this.app`,
/// direct receiver aliases, and static computed properties. Source-ordered
/// reassignment and lexical shadowing are respected; event names, handlers,
/// arguments, and other event methods are not analyzed.
pub(crate) fn rule() -> Rule {
    Rule::builder("vault.events")
        .label("Registers vault events")
        .category("vault")
        .severity(Severity::Info)
        .confidence(Confidence::High)
        .matcher(Matcher::member_call(
            MemberCallMatcher::rooted_chain("app.vault.on").arg_value(
                0,
                FlowValueMatcher::StaticExact(vec![
                    "changed".into(),
                    "created".into(),
                    "create".into(),
                    "deleted".into(),
                    "delete".into(),
                    "modified".into(),
                    "modify".into(),
                    "renamed".into(),
                    "rename".into(),
                ]),
            ),
        ))
        .build()
        .unwrap()
}
