// @case description negative fixture for js:node.network
// @tool glass-lint rules=js:node.network
// @expect-no-error glass-lint rule=js:node.network message_id=detected
function localLookalike() { return null; }
localLookalike();
import localHttp from "not-http";
// @expect-no-error glass-lint rule=js:node.network message_id=detected
localHttp;
