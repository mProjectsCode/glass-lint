use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects construction through the unshadowed global `URL` and
/// `URLSearchParams` constructors. Direct aliases retain global provenance
/// until reassigned, while local shadows and lookalikes are excluded. The
/// constructor arguments are intentionally not inspected, and static URL
/// methods or other URL-like constructors are outside this rule.
pub fn rule() -> Rule {
    Rule::builder("network.url-construction")
        .label("Constructs or references URLs")
        .category("language/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::global_constructor("URL"))
        .matcher(Matcher::global_constructor("URLSearchParams"))
        .build()
        .unwrap()
}
