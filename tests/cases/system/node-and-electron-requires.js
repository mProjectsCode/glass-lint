// @case description Ported old classifier case: minified CommonJS requires count as node and Electron imports
// @tool glass-lint rules=obsidian:filesystem.node,obsidian:electron.desktop

var f = require("fs"), e = __toESM(require("electron")); // @expect-error glass-lint rule=obsidian:filesystem.node message_id=detected
// @expect-error-after glass-lint rule=obsidian:electron.desktop message_id=detected
f.readFileSync("x");
