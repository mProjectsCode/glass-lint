// @case description negative fixture for node:node.process-environment
// @tool glass-lint rules=node:node.process-environment
// A local `process` lookalike is not Node's rooted global.
function localLookalike(process) {
    // @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
    process.env;
    // @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
    process.platform;
}
localLookalike({ env: {}, platform: "local" });

// Reassignment drops the rooted provenance from an alias.
function reassigned() {
    let nodeProcess = process;
    nodeProcess = { env: {}, platform: "local" };
    // @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
    nodeProcess.env;
}
reassigned();

// Unlisted and dynamic properties are outside the exact rooted matchers.
// @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
process.versionSnapshot;
// @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
process.memoryUsageSnapshot;
const property = getPropertyName();
// @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
process[property];
// @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
process.debugPortSnapshot;
// @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
process.getBuiltinModules("fs");

function localGlobal(global) {
    // @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
    global.process.env.LOCAL;
    // @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
    global.process.cwd();
}
localGlobal({ process: { env: {}, cwd() {} } });

function localGlobalThis(globalThis) {
    // @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
    globalThis.process.env.LOCAL;
    // @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
    globalThis.process.cwd();
}
localGlobalThis({ process: { env: {}, cwd() {} } });
