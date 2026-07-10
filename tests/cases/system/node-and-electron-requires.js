// @case description Compact CommonJS imports count as Node and Electron imports
// @tool glass-lint rules=obsidian:filesystem.node,obsidian:electron.desktop
// @tool eslint-obsidianmd config=recommended

var f = require("fs"), e = __toESM(require("electron")); // @expect-error glass-lint rule=obsidian:filesystem.node message_id=detected
// @expect-error-after glass-lint rule=obsidian:electron.desktop message_id=detected
f.readFileSync("x");
