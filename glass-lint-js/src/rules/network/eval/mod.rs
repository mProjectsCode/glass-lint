use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects direct, unshadowed global `eval` calls. Local shadowing and
/// reassignment are excluded. Function-constructor calls, rooted eval calls,
/// and `eval.call` are also covered while local lookalikes remain excluded.
pub(crate) fn rule() -> Rule {
    Rule::builder("dynamic-code.eval")
        .label("Evaluates dynamic code")
        .category("language/dynamic-code")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .matcher(Matcher::global_call("eval"))
        .matcher(Matcher::global_call("Function"))
        .matcher(Matcher::global_constructor("Function"))
        .matcher(Matcher::rooted_member_call("globalThis.eval"))
        .matcher(Matcher::rooted_member_call("window.eval"))
        .matcher(Matcher::heuristic_call("eval.call"))
        .build()
        .unwrap()
}
