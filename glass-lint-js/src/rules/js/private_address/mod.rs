//! Private-network address indicator rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects string literals containing the configured localhost, loopback,
/// wildcard, and HTTP(S) `10.*`/`192.168.*` address markers. It is a
/// medium-confidence literal heuristic rather than URL or IP parsing: it does
/// not prove network use, expand private ranges, or match partial,
/// concatenated, or dynamic values.
pub fn rule() -> Rule {
    Rule::builder("network.private-address")
        .description("References private-network addresses")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::string_contains("localhost"))
        .matcher(Matcher::string_contains("127.0.0.1"))
        .matcher(Matcher::string_contains("0.0.0.0"))
        .matcher(Matcher::string_contains("http://192.168."))
        .matcher(Matcher::string_contains("https://192.168."))
        .matcher(Matcher::string_contains("http://10."))
        .matcher(Matcher::string_contains("https://10."))
        .build()
        .unwrap()
}
