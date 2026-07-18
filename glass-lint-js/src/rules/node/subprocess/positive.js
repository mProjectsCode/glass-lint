// @case description positive fixture for node:node.subprocess
// @tool glass-lint rules=node:node.subprocess
// Both configured ESM module names are reported.
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import childProcess from "child_process";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import nodeChildProcess from "node:child_process";

// Unshadowed static CommonJS loads retain module provenance.
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
const loadedChildProcess = require("child_process");
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
const loadedNodeChildProcess = require("node:child_process");
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import workers from "node:worker_threads";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import cluster from "node:cluster";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import pty from "node-pty";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import legacyPty from "pty.js";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import execa from "execa";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import spawn from "cross-spawn";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import shell from "shelljs";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import zx from "zx";
// Additional process-launch helpers retain exact package identity.
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import npmRunPath from "npm-run-path";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import foregroundChild from "foreground-child";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import spawnCommand from "spawn-command";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import concurrently from "concurrently";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import npmRunAll from "npm-run-all";
// @expect-error glass-lint rule=node:node.subprocess message_id=detected
import sudoPrompt from "sudo-prompt";
