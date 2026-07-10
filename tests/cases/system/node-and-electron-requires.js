// @case description Compact CommonJS imports count as Node and Electron imports
// @tool glass-lint rules=js:node.filesystem,js:electron.module
// @tool eslint-obsidianmd config=recommended

var f = require("fs"), e = __toESM(require("electron")); // @expect-error glass-lint rule=js:node.filesystem message_id=detected
// @expect-error-after glass-lint rule=js:electron.module message_id=detected
f.readFileSync("x");
