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
        .matcher(Matcher::package_import("openai").unwrap())
        .matcher(Matcher::package_import("firebase").unwrap())
        .matcher(Matcher::package_import("dropbox").unwrap())
        .matcher(Matcher::package_import("@supabase/supabase-js").unwrap())
        .matcher(Matcher::package_import("@aws-sdk/client-s3").unwrap())
        .matcher(Matcher::package_import("@aws-sdk/client-dynamodb").unwrap())
        .matcher(Matcher::package_import("@aws-sdk/client-lambda").unwrap())
        .matcher(Matcher::package_import("@google-cloud/storage").unwrap())
        .matcher(Matcher::package_import("@google-cloud/firestore").unwrap())
        .matcher(Matcher::package_import("@google-cloud/pubsub").unwrap())
        .matcher(Matcher::package_import("@azure/storage-blob").unwrap())
        .matcher(Matcher::package_import("@azure/identity").unwrap())
        .matcher(Matcher::package_import("stripe").unwrap())
        .matcher(Matcher::package_import("@stripe/stripe-js").unwrap())
        .matcher(Matcher::package_import("twilio").unwrap())
        .matcher(Matcher::package_import("@twilio/voice-sdk").unwrap())
        .matcher(Matcher::package_import("@sendgrid/mail").unwrap())
        .matcher(Matcher::package_import("mailgun.js").unwrap())
        .matcher(Matcher::package_import("@octokit/rest").unwrap())
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
