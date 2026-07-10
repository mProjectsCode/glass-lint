// @case description positive fixture for js:node.process-environment
// @tool glass-lint rules=js:node.process-environment
// @expect-error glass-lint rule=js:node.process-environment message_id=detected
process.env.HOME;
// second independent example

// @expect-error glass-lint rule=js:node.process-environment message_id=detected
process.platform;

// @expect-error glass-lint rule=js:node.process-environment message_id=detected
const environment = process.env;

// @expect-no-error glass-lint rule=js:node.process-environment message_id=detected
environment;
