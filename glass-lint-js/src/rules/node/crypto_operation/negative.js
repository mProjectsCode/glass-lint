// @case description negative fixture for js:crypto.operation
// @tool glass-lint rules=js:crypto.operation
// @expect-no-error glass-lint rule=js:crypto.operation message_id=detected
// Similar module names are not configured imports.
import unrelatedCrypto from "crypto-helper";
// @expect-no-error glass-lint rule=js:crypto.operation message_id=detected
unrelatedCrypto;

// Unlisted Web Crypto methods are outside the heuristic matcher.
crypto.subtle.sign("HMAC", key);

// A shadowed CommonJS loader is not treated as a module import.
const require = () => unrelatedCrypto;
require("crypto");
