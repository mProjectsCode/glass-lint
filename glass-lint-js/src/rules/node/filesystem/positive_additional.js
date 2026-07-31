// @case description additional Node path coverage for node:node.filesystem
// @tool glass-lint rules=node:node.filesystem
// Importing a path module alone is not a filesystem operation.
import path from "path";
const loadedPath = require("path");
// @expect-no-error glass-lint rule=node:node.filesystem
path.join("root", "file.txt");
// @expect-no-error glass-lint rule=node:node.filesystem
loadedPath.resolve("root", "file.txt");
