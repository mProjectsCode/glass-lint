// @case description positive fixture for node:crypto.operation
// @tool glass-lint rules=node:crypto.operation
// @expect-error glass-lint rule=node:crypto.operation message_id=detected
import c from "node:crypto";
// Each configured module specifier is reported at import time.
// @expect-error glass-lint rule=node:crypto.operation message_id=detected
import coreCrypto from "crypto";
// @expect-error glass-lint rule=node:crypto.operation message_id=detected
import cryptoJs from "crypto-js";
// Syntactic Web Crypto calls are reported without module provenance.
// @expect-error glass-lint rule=node:crypto.operation message_id=detected
crypto.subtle.digest("SHA-256", bytes);
