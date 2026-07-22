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
        .declaration(MatcherDecl::builder().string_contains("User-Agent").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("user-agent").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("USER-AGENT").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("Authorization").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("authorization").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("AUTHORIZATION").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("Cookie").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("COOKIE").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("Set-Cookie").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("SET-COOKIE").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("Proxy-Authorization").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("PROXY-AUTHORIZATION").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("X-API-Key").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("x-api-key").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("Api-Key").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("api-key").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("API-KEY").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("X-Auth-Token").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("x-auth-token").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("X-Access-Token").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("x-access-token").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("X-Client-Token").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("x-client-token").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("X-API-Token").build().expect("valid matcher declaration"))
        .declaration(MatcherDecl::builder().string_contains("x-api-token").build().expect("valid matcher declaration"))
        .build()
        .unwrap()
}
