// @case description positive fixture for node:node.filesystem
// @tool glass-lint rules=node:node.filesystem
// Filesystem modules are reported at module load; path modules are reported
// when a path API is actually called.
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import fs from "fs";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import promises from "fs/promises";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import nodeFs from "node:fs";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import nodePromises from "node:fs/promises";
import nodePath from "node:path";
import path from "path";
// Path imports are classified when an API is actually called.
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
nodePath.join("root", "file.txt");
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
path.resolve("root", "file.txt");
// Unshadowed static CommonJS loads retain module provenance.
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
const loadedFs = require("fs");
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
const loadedPromises = require("node:fs/promises");
const loadedPath = require("path");
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
loadedPath.basename("/tmp/file.txt");
// Named path imports retain module provenance.
import { relative } from "node:path";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
relative("a", "b");
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import extra from "fs-extra";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import graceful from "graceful-fs";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import memoryFs from "memfs";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import union from "unionfs";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import watcher from "chokidar";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import lockfile from "proper-lockfile";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import temporary from "tmp";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import temporaryPromise from "tmp-promise";
// Common filesystem helpers retain exact package identity.
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import rimraf from "rimraf";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import mkdirp from "mkdirp";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import makeDir from "make-dir";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import writeFileAtomic from "write-file-atomic";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import fsMonkey from "fs-monkey";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import mockFs from "mock-fs";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import watchpack from "watchpack";
// @expect-error glass-lint rule=node:node.filesystem message_id=detected
import fsevents from "fsevents";
