// @case description negative fixture for node:node.process-environment
// @tool glass-lint rules=node:node.process-environment
// A local `process` lookalike is not Node's rooted global.
function localLookalike(process) {
    // @expect-no-error glass-lint rule=node:node.process-environment
    process.env;
    // @expect-no-error glass-lint rule=node:node.process-environment
    process.platform;
}
localLookalike({ env: {}, platform: "local" });

// Reassignment drops the rooted provenance from an alias.
function reassigned() {
    let nodeProcess = process;
    nodeProcess = { env: {}, platform: "local" };
    // @expect-no-error glass-lint rule=node:node.process-environment
    nodeProcess.env;
}
reassigned();

// Unlisted and dynamic properties are outside the exact rooted matchers.
// @expect-no-error glass-lint rule=node:node.process-environment
process.versionSnapshot;
// @expect-no-error glass-lint rule=node:node.process-environment
process.memoryUsageSnapshot;
const property = getPropertyName();
// @expect-no-error glass-lint rule=node:node.process-environment
process[property];
// @expect-no-error glass-lint rule=node:node.process-environment
process.debugPortSnapshot;
// @expect-no-error glass-lint rule=node:node.process-environment
process.getBuiltinModules("fs");

function localGlobal(global) {
    // @expect-no-error glass-lint rule=node:node.process-environment
    global.process.env.LOCAL;
    // @expect-no-error glass-lint rule=node:node.process-environment
    global.process.cwd();
}
localGlobal({ process: { env: {}, cwd() {} } });

function localGlobalThis(globalThis) {
    // @expect-no-error glass-lint rule=node:node.process-environment
    globalThis.process.env.LOCAL;
    // @expect-no-error glass-lint rule=node:node.process-environment
    globalThis.process.cwd();
}
localGlobalThis({ process: { env: {}, cwd() {} } });
