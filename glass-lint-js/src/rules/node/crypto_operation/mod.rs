//! Node and Web Crypto operation rule definition.

use glass_lint_core::rules::{Category, Confidence, EventQuery, Rule, Severity};

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
        .query(EventQuery::import_exact("crypto"))
        .query(EventQuery::import_exact("crypto/promises"))
        .query(EventQuery::import_exact("node:crypto"))
        .query(EventQuery::import_exact("node:crypto/promises"))
        .query(EventQuery::import_package("crypto-js"))
        .query(EventQuery::import_package("@noble/hashes"))
        .query(EventQuery::import_package("@noble/curves"))
        .query(EventQuery::import_package("tweetnacl"))
        .query(EventQuery::import_package("libsodium-wrappers"))
        .query(EventQuery::import_package("jose"))
        .query(EventQuery::import_package("jsonwebtoken"))
        .query(EventQuery::import_package("node-forge"))
        .query(EventQuery::import_package("elliptic"))
        .query(EventQuery::import_package("bcrypt"))
        .query(EventQuery::import_package("bcryptjs"))
        .query(EventQuery::import_package("argon2"))
        .query(EventQuery::import_package("scrypt-js"))
        .query(EventQuery::member_call_rooted("crypto.subtle.digest"))
        .query(EventQuery::member_call_rooted("crypto.subtle.encrypt"))
        .query(EventQuery::member_call_rooted("crypto.subtle.decrypt"))
        .query(EventQuery::member_call_rooted("crypto.subtle.sign"))
        .query(EventQuery::member_call_rooted("crypto.subtle.verify"))
        .query(EventQuery::member_call_rooted("crypto.subtle.deriveBits"))
        .query(EventQuery::member_call_rooted("crypto.subtle.deriveKey"))
        .query(EventQuery::member_call_rooted("crypto.subtle.generateKey"))
        .query(EventQuery::member_call_rooted("crypto.subtle.importKey"))
        .query(EventQuery::member_call_rooted("crypto.subtle.exportKey"))
        .query(EventQuery::member_call_rooted("crypto.subtle.wrapKey"))
        .query(EventQuery::member_call_rooted("crypto.subtle.unwrapKey"))
        .query(EventQuery::member_call_rooted(
            "globalThis.crypto.subtle.digest",
        ))
        .query(EventQuery::member_call_rooted(
            "globalThis.crypto.subtle.encrypt",
        ))
        .query(EventQuery::member_call_rooted(
            "globalThis.crypto.subtle.decrypt",
        ))
        .query(EventQuery::member_call_rooted("webcrypto.subtle.digest"))
        .query(EventQuery::member_call_rooted("webcrypto.subtle.encrypt"))
        .query(EventQuery::member_call_rooted("webcrypto.subtle.decrypt"))
        .query(EventQuery::member_call_module("crypto", "createHash"))
        .query(EventQuery::member_call_module("node:crypto", "createHash"))
        .query(EventQuery::member_call_module("crypto", "createCipheriv"))
        .query(EventQuery::member_call_module(
            "node:crypto",
            "createCipheriv",
        ))
        .query(EventQuery::member_call_module("crypto", "generateKeyPair"))
        .query(EventQuery::member_call_module(
            "node:crypto",
            "generateKeyPair",
        ))
        .query(EventQuery::member_call_module("crypto", "sign"))
        .query(EventQuery::member_call_module("node:crypto", "sign"))
        .build()
        .unwrap()
}
