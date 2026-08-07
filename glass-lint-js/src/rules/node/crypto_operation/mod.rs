//! Node and Web Crypto operation rule definition.

use glass_lint_core::rules::{Confidence, EventQuery, Rule, Severity};

const CRYPTO_MODULES: &[&str] = &[
    "crypto",
    "crypto/promises",
    "node:crypto",
    "node:crypto/promises",
];
const CRYPTO_PACKAGES: &[&str] = &[
    "crypto-js",
    "@noble/hashes",
    "@noble/curves",
    "tweetnacl",
    "libsodium-wrappers",
    "jose",
    "jsonwebtoken",
    "node-forge",
    "elliptic",
    "bcrypt",
    "bcryptjs",
    "argon2",
    "scrypt-js",
];
const CRYPTO_ROOTED_CALLS: &[&str] = &[
    "crypto.subtle.digest",
    "crypto.subtle.encrypt",
    "crypto.subtle.decrypt",
    "crypto.subtle.sign",
    "crypto.subtle.verify",
    "crypto.subtle.deriveBits",
    "crypto.subtle.deriveKey",
    "crypto.subtle.generateKey",
    "crypto.subtle.importKey",
    "crypto.subtle.exportKey",
    "crypto.subtle.wrapKey",
    "crypto.subtle.unwrapKey",
    "globalThis.crypto.subtle.digest",
    "globalThis.crypto.subtle.encrypt",
    "globalThis.crypto.subtle.decrypt",
    "webcrypto.subtle.digest",
    "webcrypto.subtle.encrypt",
    "webcrypto.subtle.decrypt",
];
const CRYPTO_MODULE_CALLS: &[(&str, &str)] = &[
    ("crypto", "createHash"),
    ("node:crypto", "createHash"),
    ("crypto", "createCipheriv"),
    ("node:crypto", "createCipheriv"),
    ("crypto", "generateKeyPair"),
    ("node:crypto", "generateKeyPair"),
    ("crypto", "sign"),
    ("node:crypto", "sign"),
];

/// Detects imports of the Node crypto modules and configured cryptographic
/// libraries, plus rooted Web Crypto operation calls. Import reports are
/// intentionally emitted at the import rather than later API use.
pub fn rule() -> Rule {
    Rule::builder("crypto.operation")
        .description("Uses cryptographic operations")
        .severity(Severity::Info)
        .confidence(Confidence::Medium)
        .queries(CRYPTO_MODULES.iter().copied().map(EventQuery::import_exact))
        .queries(
            CRYPTO_PACKAGES
                .iter()
                .copied()
                .map(EventQuery::import_package),
        )
        .queries(
            CRYPTO_ROOTED_CALLS
                .iter()
                .copied()
                .map(EventQuery::member_call_rooted),
        )
        .queries(
            CRYPTO_MODULE_CALLS
                .iter()
                .copied()
                .map(|pair| EventQuery::member_call_module(pair.0, pair.1)),
        )
        .build()
        .unwrap()
}
