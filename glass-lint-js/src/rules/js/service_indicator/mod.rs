//! Service and SDK endpoint indicator rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

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
        .matcher(Matcher::import("openai"))
        .matcher(Matcher::import("firebase"))
        .matcher(Matcher::import("dropbox"))
        .matcher(Matcher::import("@supabase/supabase-js"))
        .matcher(Matcher::import("@aws-sdk/client-s3"))
        .matcher(Matcher::import("@aws-sdk/client-dynamodb"))
        .matcher(Matcher::import("@aws-sdk/client-lambda"))
        .matcher(Matcher::import("@google-cloud/storage"))
        .matcher(Matcher::import("@google-cloud/firestore"))
        .matcher(Matcher::import("@google-cloud/pubsub"))
        .matcher(Matcher::import("@azure/storage-blob"))
        .matcher(Matcher::import("@azure/identity"))
        .matcher(Matcher::import("stripe"))
        .matcher(Matcher::import("@stripe/stripe-js"))
        .matcher(Matcher::import("twilio"))
        .matcher(Matcher::import("@twilio/voice-sdk"))
        .matcher(Matcher::import("@sendgrid/mail"))
        .matcher(Matcher::import("mailgun.js"))
        .matcher(Matcher::import("@octokit/rest"))
        .matcher(Matcher::string_contains("api.openai.com"))
        .matcher(Matcher::string_contains("amazonaws.com"))
        .matcher(Matcher::string_contains("supabase.co"))
        .matcher(Matcher::string_contains("api.stripe.com"))
        .matcher(Matcher::string_contains("api.twilio.com"))
        .matcher(Matcher::string_contains("api.sendgrid.com"))
        .matcher(Matcher::string_contains("api.mailgun.net"))
        .matcher(Matcher::string_contains("slack.com/api"))
        .build()
        .unwrap()
}
