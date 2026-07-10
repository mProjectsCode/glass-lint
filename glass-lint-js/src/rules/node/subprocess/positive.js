// @case description positive fixture for js:node.subprocess
// @tool glass-lint rules=js:node.subprocess
// @expect-error glass-lint rule=js:node.subprocess message_id=detected
import cp from "node:child_process";
// second independent example
// @expect-error glass-lint rule=js:node.subprocess message_id=detected
import * as secondChildProcess from "node:child_process";
