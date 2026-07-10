// @case description positive fixture for js:crypto.operation
// @tool glass-lint rules=js:crypto.operation
// @expect-error glass-lint rule=js:crypto.operation message_id=detected
import c from "node:crypto";
// second independent example
// @expect-error glass-lint rule=js:crypto.operation message_id=detected
import * as secondCrypto from "node:crypto";
