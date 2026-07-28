//! Node and Web Crypto operation rule definition.

use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};

/// Detects imports of the Node crypto modules and configured cryptographic
/// libraries, plus rooted Web Crypto operation calls. Import reports are
/// intentionally emitted at the import rather than later API use.
#[allow(clippy::too_many_lines)]
pub fn rule() -> Rule {
    Rule::builder("crypto.operation")
        .description("Uses cryptographic operations")
        .category(Category::new("language/crypto").unwrap())
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .query(QueryDecl::import_exact("crypto"))
        .query(QueryDecl::import_exact("crypto/promises"))
        .query(QueryDecl::import_exact("node:crypto"))
        .query(QueryDecl::import_exact("node:crypto/promises"))
        .query(QueryDecl::import_package("crypto-js"))
        .query(QueryDecl::import_package("@noble/hashes"))
        .query(QueryDecl::import_package("@noble/curves"))
        .query(QueryDecl::import_package("tweetnacl"))
        .query(QueryDecl::import_package("libsodium-wrappers"))
        .query(QueryDecl::import_package("jose"))
        .query(QueryDecl::import_package("jsonwebtoken"))
        .query(QueryDecl::import_package("node-forge"))
        .query(QueryDecl::import_package("elliptic"))
        .query(QueryDecl::import_package("bcrypt"))
        .query(QueryDecl::import_package("bcryptjs"))
        .query(QueryDecl::import_package("argon2"))
        .query(QueryDecl::import_package("scrypt-js"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.digest"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.encrypt"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.decrypt"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.sign"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.verify"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.deriveBits"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.deriveKey"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.generateKey"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.importKey"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.exportKey"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.wrapKey"))
        .query(QueryDecl::member_call_rooted("crypto.subtle.unwrapKey"))
        .build()
        .unwrap()
}
