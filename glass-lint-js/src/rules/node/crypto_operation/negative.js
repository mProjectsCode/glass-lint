// @case description negative fixture for node:crypto.operation
// @tool glass-lint rules=node:crypto.operation
// @expect-no-error glass-lint rule=node:crypto.operation
// Similar module names are not configured imports.
import unrelatedCrypto from "crypto-helper";
// Package boundaries remain exact; similarly named libraries are excluded.
// @expect-no-error glass-lint rule=node:crypto.operation
import helper from "@noble/hashes-helper";
// @expect-no-error glass-lint rule=node:crypto.operation
import localJwt from "jsonwebtoken-helper";
// @expect-no-error glass-lint rule=node:crypto.operation
unrelatedCrypto;

// Unlisted Web Crypto methods are outside the heuristic matcher.
crypto.subtle.randomOperation("value");

function localCrypto(crypto) {
    // @expect-no-error glass-lint rule=node:crypto.operation
    crypto.subtle.digest("SHA-256", bytes);
}

// A shadowed CommonJS loader is not treated as a module import.
const require = () => unrelatedCrypto;
require("crypto");

function localGlobal(global) {
    // @expect-no-error glass-lint rule=node:crypto.operation
    global.crypto.subtle.digest("SHA-256", bytes);
}
localGlobal({ crypto: { subtle: { digest() {} } } });
