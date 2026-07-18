//! Node and Web Crypto operation rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects imports of the Node crypto modules and configured cryptographic
/// libraries, plus syntactic `crypto.subtle` operation calls. Import reports
/// are intentionally emitted at the import rather than later API use; the
/// heuristic Web Crypto chains can match same-shaped local bindings.
pub fn rule() -> Rule {
    Rule::builder("crypto.operation")
        .description("Uses cryptographic operations")
        .category("language/crypto")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .matcher(Matcher::import("crypto"))
        .matcher(Matcher::import("crypto/promises"))
        .matcher(Matcher::import("node:crypto"))
        .matcher(Matcher::import("node:crypto/promises"))
        .matcher(Matcher::import("crypto-js"))
        .matcher(Matcher::import("@noble/hashes"))
        .matcher(Matcher::import("@noble/curves"))
        .matcher(Matcher::import("tweetnacl"))
        .matcher(Matcher::import("libsodium-wrappers"))
        .matcher(Matcher::import("jose"))
        .matcher(Matcher::import("jsonwebtoken"))
        .matcher(Matcher::import("node-forge"))
        .matcher(Matcher::import("elliptic"))
        .matcher(Matcher::import("bcrypt"))
        .matcher(Matcher::import("bcryptjs"))
        .matcher(Matcher::import("argon2"))
        .matcher(Matcher::import("scrypt-js"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.digest"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.encrypt"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.decrypt"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.sign"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.verify"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.deriveBits"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.deriveKey"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.generateKey"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.importKey"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.exportKey"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.wrapKey"))
        .matcher(Matcher::heuristic_member_call("crypto.subtle.unwrapKey"))
        .matcher(Matcher::rooted_member_call("global.crypto.subtle.digest"))
        .matcher(Matcher::rooted_member_call("global.crypto.subtle.encrypt"))
        .matcher(Matcher::rooted_member_call("global.crypto.subtle.decrypt"))
        .matcher(Matcher::rooted_member_call("global.crypto.subtle.sign"))
        .matcher(Matcher::rooted_member_call("global.crypto.subtle.verify"))
        .matcher(Matcher::rooted_member_call(
            "global.crypto.subtle.deriveBits",
        ))
        .matcher(Matcher::rooted_member_call(
            "global.crypto.subtle.deriveKey",
        ))
        .matcher(Matcher::rooted_member_call(
            "global.crypto.subtle.generateKey",
        ))
        .matcher(Matcher::rooted_member_call(
            "global.crypto.subtle.importKey",
        ))
        .matcher(Matcher::rooted_member_call(
            "global.crypto.subtle.exportKey",
        ))
        .matcher(Matcher::rooted_member_call("global.crypto.subtle.wrapKey"))
        .matcher(Matcher::rooted_member_call(
            "global.crypto.subtle.unwrapKey",
        ))
        .build()
        .unwrap()
}
