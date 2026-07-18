//! Node and Web Crypto operation rule definition.

use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

/// Detects imports of the Node crypto modules and configured cryptographic
/// libraries, plus rooted Web Crypto operation calls. Import reports are
/// intentionally emitted at the import rather than later API use.
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
        .matcher(Matcher::package_import("crypto-js").unwrap())
        .matcher(Matcher::package_import("@noble/hashes").unwrap())
        .matcher(Matcher::package_import("@noble/curves").unwrap())
        .matcher(Matcher::package_import("tweetnacl").unwrap())
        .matcher(Matcher::package_import("libsodium-wrappers").unwrap())
        .matcher(Matcher::package_import("jose").unwrap())
        .matcher(Matcher::package_import("jsonwebtoken").unwrap())
        .matcher(Matcher::package_import("node-forge").unwrap())
        .matcher(Matcher::package_import("elliptic").unwrap())
        .matcher(Matcher::package_import("bcrypt").unwrap())
        .matcher(Matcher::package_import("bcryptjs").unwrap())
        .matcher(Matcher::package_import("argon2").unwrap())
        .matcher(Matcher::package_import("scrypt-js").unwrap())
        .matcher(Matcher::rooted_member_call("crypto.subtle.digest"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.encrypt"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.decrypt"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.sign"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.verify"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.deriveBits"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.deriveKey"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.generateKey"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.importKey"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.exportKey"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.wrapKey"))
        .matcher(Matcher::rooted_member_call("crypto.subtle.unwrapKey"))
        .build()
        .unwrap()
}
