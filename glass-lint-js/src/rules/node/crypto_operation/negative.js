// @case description negative fixture for js:crypto.operation
// @tool glass-lint rules=js:crypto.operation
// @expect-no-error glass-lint rule=js:crypto.operation message_id=detected
function localLookalike() { return null; }
localLookalike();
import unrelatedCrypto from "crypto-helper";

// @expect-no-error glass-lint rule=js:crypto.operation message_id=detected
unrelatedCrypto;
