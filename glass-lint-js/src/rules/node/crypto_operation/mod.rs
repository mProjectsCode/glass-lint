//! Node and Web Crypto operation rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects imports of the Node crypto modules and configured cryptographic
/// libraries, plus rooted Web Crypto operation calls. Import reports are
/// intentionally emitted at the import rather than later API use.
pub fn rule() -> Rule {
    Rule::builder("crypto.operation")
        .description("Uses cryptographic operations")
        .category("language/crypto")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(MatcherDecl::import("crypto"))
        .declaration(MatcherDecl::import("crypto/promises"))
        .declaration(MatcherDecl::import("node:crypto"))
        .declaration(MatcherDecl::import("node:crypto/promises"))
        .declaration(MatcherDecl::package_import("crypto-js"))
        .declaration(MatcherDecl::package_import("@noble/hashes"))
        .declaration(MatcherDecl::package_import("@noble/curves"))
        .declaration(MatcherDecl::package_import("tweetnacl"))
        .declaration(MatcherDecl::package_import("libsodium-wrappers"))
        .declaration(MatcherDecl::package_import("jose"))
        .declaration(MatcherDecl::package_import("jsonwebtoken"))
        .declaration(MatcherDecl::package_import("node-forge"))
        .declaration(MatcherDecl::package_import("elliptic"))
        .declaration(MatcherDecl::package_import("bcrypt"))
        .declaration(MatcherDecl::package_import("bcryptjs"))
        .declaration(MatcherDecl::package_import("argon2"))
        .declaration(MatcherDecl::package_import("scrypt-js"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.digest"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.encrypt"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.decrypt"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.sign"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.verify"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.deriveBits"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.deriveKey"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.generateKey"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.importKey"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.exportKey"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.wrapKey"))
        .declaration(MatcherDecl::rooted_member_call("crypto.subtle.unwrapKey"))
        .build()
        .unwrap()
}
