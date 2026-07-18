//! Header-marker indicator rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects string literals containing the configured `Authorization` and
/// `User-Agent` marker substrings. This is an opt-in heuristic indicator: it
/// does not prove that a literal is used as a request header, does not parse
/// computed or concatenated values, and intentionally excludes other casing
/// and unrelated lookalike prose.
pub fn rule() -> Rule {
    Rule::builder("network.header-indicator")
        .description("References authorization or user-agent headers")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::string_contains("User-Agent"))
        .matcher(Matcher::string_contains("user-agent"))
        .matcher(Matcher::string_contains("USER-AGENT"))
        .matcher(Matcher::string_contains("Authorization"))
        .matcher(Matcher::string_contains("authorization"))
        .matcher(Matcher::string_contains("AUTHORIZATION"))
        .matcher(Matcher::string_contains("Cookie"))
        .matcher(Matcher::string_contains("COOKIE"))
        .matcher(Matcher::string_contains("Set-Cookie"))
        .matcher(Matcher::string_contains("SET-COOKIE"))
        .matcher(Matcher::string_contains("Proxy-Authorization"))
        .matcher(Matcher::string_contains("PROXY-AUTHORIZATION"))
        .matcher(Matcher::string_contains("X-API-Key"))
        .matcher(Matcher::string_contains("x-api-key"))
        .matcher(Matcher::string_contains("Api-Key"))
        .matcher(Matcher::string_contains("api-key"))
        .matcher(Matcher::string_contains("API-KEY"))
        .matcher(Matcher::string_contains("X-Auth-Token"))
        .matcher(Matcher::string_contains("x-auth-token"))
        .matcher(Matcher::string_contains("X-Access-Token"))
        .matcher(Matcher::string_contains("x-access-token"))
        .matcher(Matcher::string_contains("X-Client-Token"))
        .matcher(Matcher::string_contains("x-client-token"))
        .matcher(Matcher::string_contains("X-API-Token"))
        .matcher(Matcher::string_contains("x-api-token"))
        .build()
        .unwrap()
}
