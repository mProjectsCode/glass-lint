// @case description additional Node path coverage for node:node.filesystem
// @tool glass-lint rules=node:node.filesystem
// The remaining configured path module names are reported.
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import path from "path";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
const loadedPath = require("path");
