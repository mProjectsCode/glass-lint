// @case description negative fixture for js:node.process-environment
// @tool glass-lint rules=js:node.process-environment
// A local `process` lookalike is not Node's rooted global.
function localLookalike(process) {
    // @expect-no-error glass-lint rule=js:node.process-environment message_id=detected
    process.env;
    // @expect-no-error glass-lint rule=js:node.process-environment message_id=detected
    process.platform;
}
localLookalike({ env: {}, platform: "local" });

// Reassignment drops the rooted provenance from an alias.
function reassigned() {
    let nodeProcess = process;
    nodeProcess = { env: {}, platform: "local" };
    // @expect-no-error glass-lint rule=js:node.process-environment message_id=detected
    nodeProcess.env;
}
reassigned();

// Unlisted and dynamic properties are outside the exact rooted matchers.
// @expect-no-error glass-lint rule=js:node.process-environment message_id=detected
process.version;
const property = getPropertyName();
// @expect-no-error glass-lint rule=js:node.process-environment message_id=detected
process[property];
