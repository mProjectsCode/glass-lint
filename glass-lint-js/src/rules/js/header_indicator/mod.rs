//! Header-marker indicator rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

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
        // Sink-associated coverage proves header names in request option
        // objects; literal matchers below intentionally retain this rule's
        // source-wide heuristic policy.
        .declaration(
            MatcherDecl::builder()
                .call_global("fetch")
                .arg_object_keys(
                    1,
                    [
                        "User-Agent",
                        "user-agent",
                        "Authorization",
                        "authorization",
                        "Cookie",
                        "cookie",
                        "X-API-Key",
                        "x-api-key",
                    ],
                )
                .build()
                .unwrap(),
        )
        .declaration(MatcherDecl::string_contains("User-Agent"))
        .declaration(MatcherDecl::string_contains("user-agent"))
        .declaration(MatcherDecl::string_contains("USER-AGENT"))
        .declaration(MatcherDecl::string_contains("Authorization"))
        .declaration(MatcherDecl::string_contains("authorization"))
        .declaration(MatcherDecl::string_contains("AUTHORIZATION"))
        .declaration(MatcherDecl::string_contains("Cookie"))
        .declaration(MatcherDecl::string_contains("COOKIE"))
        .declaration(MatcherDecl::string_contains("Set-Cookie"))
        .declaration(MatcherDecl::string_contains("SET-COOKIE"))
        .declaration(MatcherDecl::string_contains("Proxy-Authorization"))
        .declaration(MatcherDecl::string_contains("PROXY-AUTHORIZATION"))
        .declaration(MatcherDecl::string_contains("X-API-Key"))
        .declaration(MatcherDecl::string_contains("x-api-key"))
        .declaration(MatcherDecl::string_contains("Api-Key"))
        .declaration(MatcherDecl::string_contains("api-key"))
        .declaration(MatcherDecl::string_contains("API-KEY"))
        .declaration(MatcherDecl::string_contains("X-Auth-Token"))
        .declaration(MatcherDecl::string_contains("x-auth-token"))
        .declaration(MatcherDecl::string_contains("X-Access-Token"))
        .declaration(MatcherDecl::string_contains("x-access-token"))
        .declaration(MatcherDecl::string_contains("X-Client-Token"))
        .declaration(MatcherDecl::string_contains("x-client-token"))
        .declaration(MatcherDecl::string_contains("X-API-Token"))
        .declaration(MatcherDecl::string_contains("x-api-token"))
        .build()
        .unwrap()
}
