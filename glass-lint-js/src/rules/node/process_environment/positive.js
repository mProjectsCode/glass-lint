// @case description positive fixture for js:node.process-environment
// @tool glass-lint rules=js:node.process-environment
// Direct reads of both configured properties are detected.
// @expect-error glass-lint rule=js:node.process-environment message_id=detected
process.env.HOME;
// @expect-error glass-lint rule=js:node.process-environment message_id=detected
process.platform;

// The root and the configured member can be aliased without losing
// provenance.
// @expect-error glass-lint rule=js:node.process-environment message_id=detected
const environment = process.env;
// @expect-no-error glass-lint rule=js:node.process-environment message_id=detected
environment;
const nodeProcess = process;
// @expect-error glass-lint rule=js:node.process-environment message_id=detected
nodeProcess.env.PATH;
const platformProcess = process;
// @expect-error glass-lint rule=js:node.process-environment message_id=detected
platformProcess.platform;

// Static computed access resolves to the same configured member.
// @expect-error glass-lint rule=js:node.process-environment message_id=detected
process["env"];
// @expect-error glass-lint rule=js:node.process-environment message_id=detected
process["platform"];
