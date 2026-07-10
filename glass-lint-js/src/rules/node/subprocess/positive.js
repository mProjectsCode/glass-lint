// @case description positive fixture for js:node.subprocess
// @tool glass-lint rules=js:node.subprocess
// Both configured ESM module names are reported.
// @expect-error glass-lint rule=js:node.subprocess message_id=detected
import childProcess from "child_process";
// @expect-error glass-lint rule=js:node.subprocess message_id=detected
import nodeChildProcess from "node:child_process";

// Unshadowed static CommonJS loads retain module provenance.
// @expect-error glass-lint rule=js:node.subprocess message_id=detected
const loadedChildProcess = require("child_process");
// @expect-error glass-lint rule=js:node.subprocess message_id=detected
const loadedNodeChildProcess = require("node:child_process");
