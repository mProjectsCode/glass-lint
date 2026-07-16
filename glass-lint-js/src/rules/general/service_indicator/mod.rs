//! Service and SDK endpoint indicator rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects static ESM or unshadowed CommonJS loads of the listed service SDKs
/// and string literals containing configured service endpoint markers. Module
/// matches use exact module provenance; literal matches are medium-confidence
/// substring heuristics over literals and template quasis, so they do not
/// prove network use or reconstruct arbitrary concatenated or dynamic values.
pub fn rule() -> Rule {
    Rule::builder("network.service-indicator")
        .label("References service or SDK endpoints")
        .category("browser/network")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::import("openai"))
        .matcher(Matcher::import("firebase"))
        .matcher(Matcher::import("dropbox"))
        .matcher(Matcher::import("@supabase/supabase-js"))
        .matcher(Matcher::string_literal("api.openai.com"))
        .matcher(Matcher::string_literal("amazonaws.com"))
        .matcher(Matcher::string_literal("supabase.co"))
        .build()
        .unwrap()
}
