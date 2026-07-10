// @case description additional Node path coverage for js:node.filesystem
// @tool glass-lint rules=js:node.filesystem
// The remaining configured path module names are reported.
// @expect-error glass-lint rule=js:node.filesystem message_id=detected
import path from "path";
// @expect-error glass-lint rule=js:node.filesystem message_id=detected
const loadedPath = require("path");
