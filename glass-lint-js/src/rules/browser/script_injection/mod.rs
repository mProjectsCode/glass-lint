//! Browser executable-script-injection rule definition.

use glass_lint_core::rules::{
    Confidence, Matcher, MemberCallMatcher, Rule, Severity, ValueMatcher,
};

/// Detects `document.createElement` calls whose tag argument resolves to
/// `script`, including constant string concatenation and aliases of
/// `createElement`. It reports creation itself, without requiring insertion or
/// executable content. Rooted document identity rejects local lookalikes;
/// dynamic values and other static tags are excluded.
pub fn rule() -> Rule {
    Rule::builder("dynamic-code.script-injection")
        .description("Injects executable script elements")
        .category("browser/dom")
        .confidence(Confidence::Medium)
        .severity(Severity::Warning)
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("document.createElement").arg_static_strings(0, ["script"]),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("window.document.createElement")
                .arg_static_strings(0, ["script"]),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("globalThis.document.createElement")
                .arg_static_strings(0, ["script"]),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("document.write").arg(
                0,
                ValueMatcher::static_string().contains_any(["<script", "javascript:"]),
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("window.document.write").arg(
                0,
                ValueMatcher::static_string().contains_any(["<script", "javascript:"]),
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("globalThis.document.write").arg(
                0,
                ValueMatcher::static_string().contains_any(["<script", "javascript:"]),
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("document.writeln").arg(
                0,
                ValueMatcher::static_string().contains_any(["<script", "javascript:"]),
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("window.document.writeln").arg(
                0,
                ValueMatcher::static_string().contains_any(["<script", "javascript:"]),
            ),
        ))
        .matcher(Matcher::from(
            MemberCallMatcher::rooted("globalThis.document.writeln").arg(
                0,
                ValueMatcher::static_string().contains_any(["<script", "javascript:"]),
            ),
        ))
        .build()
        .unwrap()
}
