#!/usr/bin/env node
import { ESLint } from "eslint";
import obsidianmdImport from "eslint-plugin-obsidianmd";

const plugin = obsidianmdImport.default ?? obsidianmdImport;
let input = "";
for await (const chunk of process.stdin) input += chunk;
const request = JSON.parse(input);
if (request.protocol_version !== 1) throw new Error(`unsupported protocol ${request.protocol_version}`);

function collectRules(obj, prefix) {
  const result = [];
  for (const [key, val] of Object.entries(obj)) {
    if (val && typeof val === "object" && val.meta) {
      result.push(prefix + key);
    } else if (val && typeof val === "object") {
      result.push(...collectRules(val, prefix + key + "/"));
    }
  }
  return result;
}

function mapRuleId(ruleId) {
  if (!ruleId) return "eslint-config:unknown";
  if (ruleId.startsWith("obsidianmd/")) {
    return `eslint-obsidianmd:${ruleId.slice("obsidianmd/".length)}`;
  }
  const normalized = ruleId
    .replace(/^@/, "")
    .replace(/\//g, ".");
  return `eslint-config:${normalized}`;
}

let eslint;
if (request.config) {
  const configArray = plugin.configs[request.config];
  if (!configArray) throw new Error(`unknown config ${request.config}`);
  eslint = new ESLint({
    overrideConfigFile: true,
    overrideConfig: configArray,
  });
} else {
  const allRules = collectRules(plugin.rules ?? {}, "obsidianmd/");
  const configuredRules = {};
  if (request.rules.includes("*")) {
    for (const id of allRules) configuredRules[id] = "error";
  } else {
    for (const id of request.rules) {
      if (!allRules.includes(id)) throw new Error(`unknown rule ${id}`);
      configuredRules[id] = "error";
    }
  }

  const globals = Object.fromEntries([
    "window", "document", "navigator", "console", "fetch", "setTimeout",
    "clearTimeout", "setInterval", "clearInterval", "globalThis",
    ...(request.context?.globals ?? []),
  ].map(name => [name, "readonly"]));

  eslint = new ESLint({
    overrideConfigFile: true,
    overrideConfig: [{
      files: ["**/*.js", "**/*.jsx"],
      languageOptions: { ecmaVersion: "latest", sourceType: "module", globals },
      plugins: { obsidianmd: plugin },
      rules: configuredRules,
    }],
  });
}

const [result] = await eslint.lintText(request.source, { filePath: request.filename });
const severity = value => value === 2 ? "error" : value === 1 ? "warning" : "info";
const findings = result.messages.map(message => ({
  rule_id: mapRuleId(message.ruleId),
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
