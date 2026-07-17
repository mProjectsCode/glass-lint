// @case description positive fixture for node:node.filesystem
// @tool glass-lint rules=node:node.filesystem
// Every configured filesystem/path module is reported at its ESM load.
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import fs from "fs";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import promises from "fs/promises";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import nodeFs from "node:fs";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import nodePromises from "node:fs/promises";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import nodePath from "node:path";

// Unshadowed static CommonJS loads retain module provenance.
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
const loadedFs = require("fs");
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
const loadedPromises = require("node:fs/promises");
