#!/usr/bin/env node
import { ESLint } from "eslint";
import obsidianmdImport from "eslint-plugin-obsidianmd";

const plugin = obsidianmdImport.default ?? obsidianmdImport;
let input = "";
for await (const chunk of process.stdin) input += chunk;
const request = JSON.parse(input);
if (request.protocol_version !== 1) throw new Error(`unsupported protocol ${request.protocol_version}`);

const configuredRules = {};
for (const id of request.rules) {
  const [namespace, name] = id.split("/", 2);
  if (namespace !== "obsidianmd" || !plugin.rules?.[name]) throw new Error(`unknown rule ${id}`);
  configuredRules[id] = "error";
}

const globals = Object.fromEntries([
  "window", "document", "navigator", "console", "fetch", "setTimeout",
  "clearTimeout", "setInterval", "clearInterval", "globalThis",
  ...(request.context?.globals ?? []),
].map(name => [name, "readonly"]));

const eslint = new ESLint({
  overrideConfigFile: true,
  overrideConfig: [{
    files: ["**/*.js", "**/*.jsx"],
    languageOptions: { ecmaVersion: "latest", sourceType: "module", globals },
    plugins: { obsidianmd: plugin },
    rules: configuredRules,
  }],
});
const [result] = await eslint.lintText(request.source, { filePath: request.filename });
const severity = value => value === 2 ? "error" : value === 1 ? "warning" : "info";
const findings = result.messages.map(message => ({
  rule_id: message.ruleId?.startsWith("obsidianmd/")
    ? `eslint-obsidianmd:${message.ruleId.slice("obsidianmd/".length)}`
    : "eslint:parse-error",
  message_id: message.messageId ?? "unknown",
  message: message.message,
  severity: severity(message.severity),
  range: {
    start: { line: message.line, column: message.column },
    end: { line: message.endLine ?? message.line, column: message.endColumn ?? message.column },
  },
  evidence: [],
}));
process.stdout.write(JSON.stringify({
  protocol_version: 1,
  tool: "eslint-obsidianmd",
  tool_version: plugin.meta?.version ?? "0.4.1",
  findings,
}));
