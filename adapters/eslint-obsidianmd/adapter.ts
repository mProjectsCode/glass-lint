#!/usr/bin/env bun

import { ESLint, type Linter } from "eslint";
import obsidianmdPlugin from "eslint-plugin-obsidianmd";

const ADAPTER_PROTOCOL_VERSION = 1;
const TOOL_NAME = "eslint-obsidianmd";
const FALLBACK_PLUGIN_VERSION = "0.4.1";

type FindingSeverity = "error" | "warning" | "info";

interface AdapterRequest {
    protocol_version: number;
    filename: string;
    source: string;
    rules: string[];
    config?: string;
    context?: {
        globals?: string[];
    };
}

interface AdapterFinding {
    rule_id: string;
    message_id: string;
    message: string;
    severity: FindingSeverity;
    range: {
        start: { line: number; column: number };
        end: { line: number; column: number };
    };
    evidence: [];
}

interface AdapterResponse {
    protocol_version: number;
    tool: string;
    tool_version: string;
    findings: AdapterFinding[];
}

/** Read one JSON request from stdin, as sent by the conformance harness. */
async function readRequest(): Promise<AdapterRequest> {
    let input = "";

    for await (const chunk of process.stdin) {
        input += chunk;
    }

    const request = JSON.parse(input) as AdapterRequest;
    if (request.protocol_version !== ADAPTER_PROTOCOL_VERSION) {
        throw new Error(
            `unsupported protocol ${request.protocol_version}`,
        );
    }

    return request;
}

/** Recursively collect rule IDs from the plugin's rule namespace. */
function collectRules(
    rules: Record<string, unknown>,
    prefix: string,
): string[] {
    const ruleIds: string[] = [];

    for (const [name, value] of Object.entries(rules)) {
        if (isRuleDefinition(value)) {
            ruleIds.push(`${prefix}${name}`);
        } else if (isObject(value)) {
            ruleIds.push(...collectRules(value, `${prefix}${name}/`));
        }
    }

    return ruleIds;
}

function isObject(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null;
}

function isRuleDefinition(
    value: unknown,
): value is { meta: unknown } {
    return isObject(value) && "meta" in value;
}

/** Convert ESLint IDs into the stable IDs used by the harness. */
function mapRuleId(ruleId: string | null): string {
    if (!ruleId) {
        return "eslint-config:unknown";
    }

    if (ruleId.startsWith("obsidianmd/")) {
        return `eslint-obsidianmd:${ruleId.slice("obsidianmd/".length)}`;
    }

    return `eslint-config:${ruleId.replace(/^@/, "").replace(/\//g, ".")}`;
}

function mapSeverity(severity: Linter.Severity): FindingSeverity {
    switch (severity) {
        case 2:
            return "error";
        case 1:
            return "warning";
        default:
            return "info";
    }
}

/** Build the flat ESLint configuration selected by a harness request. */
function createEslint(request: AdapterRequest): ESLint {
    if (request.config) {
        const configName = request.config as keyof typeof obsidianmdPlugin.configs;
        const config = obsidianmdPlugin.configs[configName];

        if (!config) {
            throw new Error(`unknown config ${request.config}`);
        }

        return new ESLint({
            overrideConfigFile: true,
            overrideConfig: config,
        });
    }

    const allRules = collectRules(obsidianmdPlugin.rules, "obsidianmd/");
    const configuredRules: Linter.RulesRecord = {};

    if (request.rules.includes("*")) {
        for (const ruleId of allRules) {
            configuredRules[ruleId] = "error";
        }
    } else {
        for (const ruleId of request.rules) {
            if (!allRules.includes(ruleId)) {
                throw new Error(`unknown rule ${ruleId}`);
            }

            configuredRules[ruleId] = "error";
        }
    }

    const globalNames = [
        "window",
        "document",
        "navigator",
        "console",
        "fetch",
        "setTimeout",
        "clearTimeout",
        "setInterval",
        "clearInterval",
        "globalThis",
        ...(request.context?.globals ?? []),
    ];
    const globals: Linter.Globals = Object.fromEntries(
        globalNames.map((name) => [name, "readonly"]),
    );

    return new ESLint({
        overrideConfigFile: true,
        overrideConfig: [
            {
                files: ["**/*.js", "**/*.jsx"],
                languageOptions: {
                    ecmaVersion: "latest",
                    sourceType: "module",
                    globals,
                },
                plugins: { obsidianmd: obsidianmdPlugin },
                rules: configuredRules,
            },
        ],
    });
}

function createFinding(message: Linter.LintMessage): AdapterFinding {
    return {
        rule_id: mapRuleId(message.ruleId),
        message_id: message.messageId ?? "unknown",
        message: message.message,
        severity: mapSeverity(message.severity),
        range: {
            start: { line: message.line, column: message.column },
            end: {
                line: message.endLine ?? message.line,
                column: message.endColumn ?? message.column,
            },
        },
        evidence: [],
    };
}

async function main(): Promise<void> {
    const request = await readRequest();
    const eslint = createEslint(request);
    const [result] = await eslint.lintText(request.source, {
        filePath: request.filename,
    });

    const response: AdapterResponse = {
        protocol_version: ADAPTER_PROTOCOL_VERSION,
        tool: TOOL_NAME,
        tool_version: obsidianmdPlugin.meta?.version ?? FALLBACK_PLUGIN_VERSION,
        findings: result.messages.map(createFinding),
    };

    process.stdout.write(JSON.stringify(response));
}

await main();
