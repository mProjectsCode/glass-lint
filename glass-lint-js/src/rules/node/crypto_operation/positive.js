// @case description positive fixture for node:crypto.operation
// @tool glass-lint rules=node:crypto.operation
// @expect-error glass-lint rule=node:crypto.operation
import c from "node:crypto";
// Node's promises entry points retain the same exact package identity.
// @expect-error glass-lint rule=node:crypto.operation
import * as cryptoPromises from "crypto/promises";
// @expect-error glass-lint rule=node:crypto.operation
import * as nodeCryptoPromises from "node:crypto/promises";
// Each configured module specifier is reported at import time.
// @expect-error glass-lint rule=node:crypto.operation
import coreCrypto from "crypto";
// @expect-error glass-lint rule=node:crypto.operation
import cryptoJs from "crypto-js";
// Common cryptographic libraries are reported at their exact package roots.
// @expect-error glass-lint rule=node:crypto.operation
import nobleHashes from "@noble/hashes";
// @expect-error glass-lint rule=node:crypto.operation
import nobleCurves from "@noble/curves";
// @expect-error glass-lint rule=node:crypto.operation
import nacl from "tweetnacl";
// @expect-error glass-lint rule=node:crypto.operation
import sodium from "libsodium-wrappers";
// @expect-error glass-lint rule=node:crypto.operation
import { compactVerify } from "jose";
// @expect-error glass-lint rule=node:crypto.operation
import jwt from "jsonwebtoken";
// @expect-error glass-lint rule=node:crypto.operation
import forge from "node-forge";
// @expect-error glass-lint rule=node:crypto.operation
import elliptic from "elliptic";
// @expect-error glass-lint rule=node:crypto.operation
import bcrypt from "bcrypt";
// @expect-error glass-lint rule=node:crypto.operation
import bcryptJs from "bcryptjs";
// @expect-error glass-lint rule=node:crypto.operation
import argon2 from "argon2";
// @expect-error glass-lint rule=node:crypto.operation
import scrypt from "scrypt-js";
// Syntactic Web Crypto calls are reported without module provenance.
// @expect-error glass-lint rule=node:crypto.operation
crypto.subtle.digest("SHA-256", bytes);
// @expect-error glass-lint rule=node:crypto.operation
crypto.subtle.sign("HMAC", key, data);
// @expect-error glass-lint rule=node:crypto.operation
crypto.subtle.verify("HMAC", key, signature, data);
// @expect-error glass-lint rule=node:crypto.operation
crypto.subtle.deriveBits(params, key, 256);
// @expect-error glass-lint rule=node:crypto.operation
crypto.subtle.deriveKey(params, key, algorithm, extractable, usages);
// @expect-error glass-lint rule=node:crypto.operation
crypto.subtle.generateKey(algorithm, true, usages);
// @expect-error glass-lint rule=node:crypto.operation
crypto.subtle.importKey(format, keyData, algorithm, true, usages);
// @expect-error glass-lint rule=node:crypto.operation
crypto.subtle.exportKey(format, key);
// @expect-error glass-lint rule=node:crypto.operation
crypto.subtle.wrapKey(format, key, wrappingKey, algorithm);
// @expect-error glass-lint rule=node:crypto.operation
crypto.subtle.unwrapKey(format, wrappedKey, unwrappingKey, algorithm, unwrappedAlgorithm, true, usages);
// Node's configured global object provides an identity-safe Web Crypto root.
// @expect-error glass-lint rule=node:crypto.operation
global.crypto.subtle.digest("SHA-256", bytes);
// @expect-error glass-lint rule=node:crypto.operation
global.crypto.subtle.generateKey(algorithm, true, usages);
