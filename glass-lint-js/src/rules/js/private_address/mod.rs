//! Private-network address indicator rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects string literals containing the configured localhost, loopback,
/// wildcard, and HTTP(S) `10.*`/`192.168.*` address markers. It is a
/// medium-confidence literal heuristic rather than URL or IP parsing: it does
/// not prove network use, expand private ranges, or match partial,
/// concatenated, or dynamic values.
pub fn rule() -> Rule {
    let mut builder = Rule::builder("network.private-address")
        .description("References private-network addresses")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(MatcherDecl::string_contains("localhost"))
        .declaration(MatcherDecl::string_contains("127.0.0.1"))
        .declaration(MatcherDecl::string_contains("http://127."))
        .declaration(MatcherDecl::string_contains("https://127."))
        .declaration(MatcherDecl::string_contains("0.0.0.0"))
        .declaration(MatcherDecl::string_contains("http://192.168."))
        .declaration(MatcherDecl::string_contains("https://192.168."))
        .declaration(MatcherDecl::string_contains("http://10."))
        .declaration(MatcherDecl::string_contains("https://10."))
        .declaration(MatcherDecl::string_contains("http://172.16."))
        .declaration(MatcherDecl::string_contains("https://172.16."))
        .declaration(MatcherDecl::string_contains("http://169.254."))
        .declaration(MatcherDecl::string_contains("https://169.254."))
        .declaration(MatcherDecl::string_contains("::1"))
        .declaration(MatcherDecl::string_contains("fc00:"))
        .declaration(MatcherDecl::string_contains("fd00:"))
        .declaration(MatcherDecl::string_contains("fe80:"));

    for octet in 17..=31 {
        for scheme in ["http://", "https://"] {
            builder = builder.declaration(MatcherDecl::string_contains(format!(
                "{scheme}172.{octet}."
            )));
        }
    }

    builder.build().unwrap()
}
