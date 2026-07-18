// @case description positive fixture for node:node.process-environment
// @tool glass-lint rules=node:node.process-environment
// Direct reads of both configured properties are detected.
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.env.HOME;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.platform;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.argv;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.execPath;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.arch;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.version;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.versions;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.release;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.pid;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.ppid;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.execArgv;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.title;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.config;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.features;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.report;
// Additional stable process metadata and host helpers are exact rooted APIs.
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.allowedNodeEnvironmentFlags;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.debugPort;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.sourceMapsEnabled;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.cwd();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.memoryUsage();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.resourceUsage();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.cpuUsage();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.uptime();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.hrtime();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.getActiveResourcesInfo();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.constrainedMemory();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.getuid();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.geteuid();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.getgid();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.getegid();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.getgroups();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.umask();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.getBuiltinModule("fs");
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process.loadEnvFile(".env");

// Configured global-object process paths retain the same identity.
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
global.process.env.HOME;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
global.process.version;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
global.process.cwd();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
global.process.memoryUsage();
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
global.process.getuid();
// The standard globalThis object preserves the configured Node process identity.
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
globalThis.process.env.HOME;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
globalThis.process.version;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
globalThis.process.cwd();

// The root and the configured member can be aliased without losing
// provenance.
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
const environment = process.env;
// @expect-no-error glass-lint rule=node:node.process-environment message_id=detected
environment;
const nodeProcess = process;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
nodeProcess.env.PATH;
const platformProcess = process;
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
platformProcess.platform;

// Static computed access resolves to the same configured member.
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process["env"];
// @expect-error glass-lint rule=node:node.process-environment message_id=detected
process["platform"];
