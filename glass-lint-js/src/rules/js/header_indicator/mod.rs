//! Header-marker indicator rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects string literals containing the configured `Authorization` and
/// `User-Agent` marker substrings. This is an opt-in heuristic indicator: it
/// does not prove that a literal is used as a request header, does not parse
/// computed or concatenated values, and intentionally excludes other casing
/// and unrelated lookalike prose.
pub fn rule() -> Rule {
    Rule::builder("network.header-indicator")
        .label("References authorization or user-agent headers")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::string_literal("User-Agent"))
        .matcher(Matcher::string_literal("user-agent"))
        .matcher(Matcher::string_literal("Authorization"))
        .build()
        .unwrap()
}
