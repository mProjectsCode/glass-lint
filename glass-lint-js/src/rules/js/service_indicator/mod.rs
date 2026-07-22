//! Service and SDK endpoint indicator rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the listed service SDKs
/// and string literals containing configured service endpoint markers. Module
/// matches use exact module provenance; literal matches are medium-confidence
/// substring heuristics over literals and template quasis, so they do not
/// prove network use or reconstruct arbitrary concatenated or dynamic values.
pub fn rule() -> Rule {
    Rule::builder("network.service-indicator")
        .description("References service or SDK endpoints")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(MatcherDecl::package_import("openai"))
        .declaration(MatcherDecl::package_import("firebase"))
        .declaration(MatcherDecl::package_import("dropbox"))
        .declaration(MatcherDecl::package_import("@supabase/supabase-js"))
        .declaration(MatcherDecl::package_import("@aws-sdk/client-s3"))
        .declaration(MatcherDecl::package_import("@aws-sdk/client-dynamodb"))
        .declaration(MatcherDecl::package_import("@aws-sdk/client-lambda"))
        .declaration(MatcherDecl::package_import("@google-cloud/storage"))
        .declaration(MatcherDecl::package_import("@google-cloud/firestore"))
        .declaration(MatcherDecl::package_import("@google-cloud/pubsub"))
        .declaration(MatcherDecl::package_import("@azure/storage-blob"))
        .declaration(MatcherDecl::package_import("@azure/identity"))
        .declaration(MatcherDecl::package_import("stripe"))
        .declaration(MatcherDecl::package_import("@stripe/stripe-js"))
        .declaration(MatcherDecl::package_import("twilio"))
        .declaration(MatcherDecl::package_import("@twilio/voice-sdk"))
        .declaration(MatcherDecl::package_import("@sendgrid/mail"))
        .declaration(MatcherDecl::package_import("mailgun.js"))
        .declaration(MatcherDecl::package_import("@octokit/rest"))
        .declaration(MatcherDecl::string_contains("api.openai.com"))
        .declaration(MatcherDecl::string_contains("amazonaws.com"))
        .declaration(MatcherDecl::string_contains("supabase.co"))
        .declaration(MatcherDecl::string_contains("api.stripe.com"))
        .declaration(MatcherDecl::string_contains("api.twilio.com"))
        .declaration(MatcherDecl::string_contains("api.sendgrid.com"))
        .declaration(MatcherDecl::string_contains("api.mailgun.net"))
        .declaration(MatcherDecl::string_contains("slack.com/api"))
        .build()
        .unwrap()
}
