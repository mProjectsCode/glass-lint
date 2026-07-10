// @case description negative fixture for js:node.subprocess
// @tool glass-lint rules=js:node.subprocess
// @expect-no-error glass-lint rule=js:node.subprocess message_id=detected
function localLookalike() { return null; }
localLookalike();
import localChildProcess from "not-child_process";

// @expect-no-error glass-lint rule=js:node.subprocess message_id=detected
localChildProcess;
