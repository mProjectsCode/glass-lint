//! Service and SDK endpoint indicator rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the listed service SDKs
/// and string literals containing configured service endpoint markers. Module
/// matches use exact module provenance; literal matches are medium-confidence
/// substring heuristics over literals and template quasis, so they do not
/// prove network use or reconstruct arbitrary concatenated or dynamic values.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("network.service-indicator")
        .description("References service or SDK endpoints")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(
            MatcherDecl::builder()
                .import_package("openai")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("firebase")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("dropbox")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@supabase/supabase-js")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@aws-sdk/client-s3")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@aws-sdk/client-dynamodb")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@aws-sdk/client-lambda")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@google-cloud/storage")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@google-cloud/firestore")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@google-cloud/pubsub")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@azure/storage-blob")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@azure/identity")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("stripe")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@stripe/stripe-js")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("twilio")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@twilio/voice-sdk")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@sendgrid/mail")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("mailgun.js")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@octokit/rest")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("api.openai.com")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("amazonaws.com")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("supabase.co")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("api.stripe.com")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("api.twilio.com")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("api.sendgrid.com")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("api.mailgun.net")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .string_contains("slack.com/api")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
