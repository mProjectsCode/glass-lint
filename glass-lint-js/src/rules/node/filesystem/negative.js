// @case description negative fixture for js:node.filesystem
// @tool glass-lint rules=js:node.filesystem
// @expect-no-error glass-lint rule=js:node.filesystem message_id=detected
function localLookalike() { return null; }
localLookalike();
import localFs from "not-fs";

// @expect-no-error glass-lint rule=js:node.filesystem message_id=detected
localFs;
