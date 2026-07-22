//! Node and Web Crypto operation rule definition.

use glass_lint_core::rules::{Confidence, MatcherDecl, Rule, Severity};

/// Detects imports of the Node crypto modules and configured cryptographic
/// libraries, plus rooted Web Crypto operation calls. Import reports are
/// intentionally emitted at the import rather than later API use.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("crypto.operation")
        .description("Uses cryptographic operations")
        .category("language/crypto")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .declaration(
            MatcherDecl::builder()
                .import_exact("crypto")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_exact("crypto/promises")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_exact("node:crypto")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_exact("node:crypto/promises")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("crypto-js")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@noble/hashes")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("@noble/curves")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("tweetnacl")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("libsodium-wrappers")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("jose")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("jsonwebtoken")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("node-forge")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("elliptic")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("bcrypt")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("bcryptjs")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("argon2")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .import_package("scrypt-js")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.digest")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.encrypt")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.decrypt")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.sign")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.verify")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.deriveBits")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.deriveKey")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.generateKey")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.importKey")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.exportKey")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.wrapKey")
                .build()
                .expect("valid matcher declaration"),
        )
        .declaration(
            MatcherDecl::builder()
                .member_call_rooted("crypto.subtle.unwrapKey")
                .build()
                .expect("valid matcher declaration"),
        )
        .build()
        .unwrap()
}
