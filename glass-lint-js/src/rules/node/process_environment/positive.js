// @case description positive fixture for node:node.process-environment
// @tool glass-lint rules=node:node.process-environment
// Direct reads of both configured properties are detected.
// @expect-error glass-lint rule=node:node.process-environment
process.env.HOME;
// @expect-error glass-lint rule=node:node.process-environment
process.platform;
// @expect-error glass-lint rule=node:node.process-environment
process.argv;
// @expect-error glass-lint rule=node:node.process-environment
process.execPath;
// @expect-error glass-lint rule=node:node.process-environment
process.arch;
// @expect-error glass-lint rule=node:node.process-environment
process.version;
// @expect-error glass-lint rule=node:node.process-environment
process.versions;
// @expect-error glass-lint rule=node:node.process-environment
process.release;
// @expect-error glass-lint rule=node:node.process-environment
process.pid;
// @expect-error glass-lint rule=node:node.process-environment
process.ppid;
// @expect-error glass-lint rule=node:node.process-environment
process.execArgv;
// @expect-error glass-lint rule=node:node.process-environment
process.title;
// @expect-error glass-lint rule=node:node.process-environment
process.config;
// @expect-error glass-lint rule=node:node.process-environment
process.features;
// @expect-error glass-lint rule=node:node.process-environment
process.report;
// Additional stable process metadata and host helpers are exact rooted APIs.
// @expect-error glass-lint rule=node:node.process-environment
process.allowedNodeEnvironmentFlags;
// @expect-error glass-lint rule=node:node.process-environment
process.debugPort;
// @expect-error glass-lint rule=node:node.process-environment
process.sourceMapsEnabled;
// @expect-error glass-lint rule=node:node.process-environment
process.cwd();
// @expect-error glass-lint rule=node:node.process-environment
process.memoryUsage();
// @expect-error glass-lint rule=node:node.process-environment
process.resourceUsage();
// @expect-error glass-lint rule=node:node.process-environment
process.cpuUsage();
// @expect-error glass-lint rule=node:node.process-environment
process.uptime();
// @expect-error glass-lint rule=node:node.process-environment
process.hrtime();
// @expect-error glass-lint rule=node:node.process-environment
process.getActiveResourcesInfo();
// @expect-error glass-lint rule=node:node.process-environment
process.constrainedMemory();
// @expect-error glass-lint rule=node:node.process-environment
process.getuid();
// @expect-error glass-lint rule=node:node.process-environment
process.geteuid();
// @expect-error glass-lint rule=node:node.process-environment
process.getgid();
// @expect-error glass-lint rule=node:node.process-environment
process.getegid();
// @expect-error glass-lint rule=node:node.process-environment
process.getgroups();
// @expect-error glass-lint rule=node:node.process-environment
process.umask();
// @expect-error glass-lint rule=node:node.process-environment
process.getBuiltinModule("fs");
// @expect-error glass-lint rule=node:node.process-environment
process.loadEnvFile(".env");

// Configured global-object process paths retain the same identity.
// @expect-error glass-lint rule=node:node.process-environment
global.process.env.HOME;
// @expect-error glass-lint rule=node:node.process-environment
global.process.version;
// @expect-error glass-lint rule=node:node.process-environment
global.process.cwd();
// @expect-error glass-lint rule=node:node.process-environment
global.process.memoryUsage();
// @expect-error glass-lint rule=node:node.process-environment
global.process.getuid();
// The standard globalThis object preserves the configured Node process identity.
// @expect-error glass-lint rule=node:node.process-environment
globalThis.process.env.HOME;
// @expect-error glass-lint rule=node:node.process-environment
globalThis.process.version;
// @expect-error glass-lint rule=node:node.process-environment
globalThis.process.cwd();

// The root and the configured member can be aliased without losing
// provenance.
// @expect-error glass-lint rule=node:node.process-environment
const environment = process.env;
// @expect-no-error glass-lint rule=node:node.process-environment
environment;
const nodeProcess = process;
// @expect-error glass-lint rule=node:node.process-environment
nodeProcess.env.PATH;
const platformProcess = process;
// @expect-error glass-lint rule=node:node.process-environment
platformProcess.platform;

// Static computed access resolves to the same configured member.
// @expect-error glass-lint rule=node:node.process-environment
process["env"];
// @expect-error glass-lint rule=node:node.process-environment
process["platform"];
