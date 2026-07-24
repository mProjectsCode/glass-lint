//! Private-network address indicator rule definition.

use glass_lint_core::rules::{Category, Confidence, MatcherDecl, Rule, Severity};

/// Detects string literals containing the configured localhost, loopback,
/// wildcard, and HTTP(S) `10.*`/`192.168.*` address markers. It is a
/// medium-confidence literal heuristic rather than URL or IP parsing: it does
/// not prove network use, expand private ranges, or match partial,
/// concatenated, or dynamic values.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    let mut builder = Rule::builder("network.private-address")
        .description("References private-network addresses")
        .category(Category::new("browser/network").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(
            MatcherDecl::builder()
                .string_contains("localhost")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("127.0.0.1")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("http://127.")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("https://127.")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("0.0.0.0")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("http://192.168.")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("https://192.168.")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("http://10.")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("https://10.")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("http://172.16.")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("https://172.16.")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("http://169.254.")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("https://169.254.")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("::1")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("fc00:")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("fd00:")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("fe80:")
                .build()
                .expect("valid matcher declaration"),
        );

    for octet in 17..=31 {
        for scheme in ["http://", "https://"] {
            builder = builder.declaration(
                MatcherDecl::builder()
                    .string_contains(format!("{scheme}172.{octet}."))
                    .build()
                    .expect("valid matcher declaration"),
            );
        }
    }

    builder.build().unwrap()
}
