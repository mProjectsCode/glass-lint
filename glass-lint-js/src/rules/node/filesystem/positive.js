// @case description positive fixture for js:node.filesystem
// @tool glass-lint rules=js:node.filesystem
// @expect-error glass-lint rule=js:node.filesystem message_id=detected
import fs from "node:fs";
// second independent example

// @expect-error glass-lint rule=js:node.filesystem message_id=detected
import * as secondFs from "node:fs";
// Migrated: system/node-and-electron-requires.js

// @expect-error glass-lint rule=js:node.filesystem message_id=detected
const fs = require("fs");
