"use strict";

// The plugin entry is intentionally self-contained: Obsidian copies one
// main.js bundle, so this file must not depend on the standalone probe.

const obsidianApi = require("obsidian");
const { Modal, Notice, Plugin } = obsidianApi;

// Obsidian loads community plugins as a single main.js bundle. Keep this
// collector embedded rather than loading the reusable CLI probe relatively.
const WELL_KNOWN_GLOBAL_OBJECT_NAMES = [
    "activeWindow",
    "global",
    "globalThis",
    "self",
    "window",
];

function collectObsidianGlobals({
    app,
    globalObject = globalThis,
    moduleExports = {},
} = {}) {
    // Inventory names and descriptors only; never serialize values or invoke
    // arbitrary getters while probing a live renderer realm.
    const errors = [];
    const candidates = [];
    addRealm(candidates, "plugin-global", globalObject);

    for (const name of WELL_KNOWN_GLOBAL_OBJECT_NAMES) {
        const value = safeRead(globalObject, name, errors, `global.${name}`);
        if (isWindowLike(value)) addRealm(candidates, `global.${name}`, value);
    }

    try {
        let leafIndex = 0;
        app.workspace.iterateAllLeaves((leaf) => {
            const source = `workspace-leaf-${leafIndex++}`;
            try {
                const realm = leaf?.view?.containerEl?.ownerDocument?.defaultView;
                if (isWindowLike(realm)) addRealm(candidates, source, realm);
            } catch (error) {
                errors.push(`${source}: ${errorMessage(error)}`);
            }
        });
    } catch (error) {
        errors.push(`iterateAllLeaves: ${errorMessage(error)}`);
    }

    const realms = candidates.map(({ sources, value }, index) => {
        let descriptors = {};
        try {
            descriptors = Object.getOwnPropertyDescriptors(value);
        } catch (error) {
            errors.push(`realm-${index} descriptors: ${errorMessage(error)}`);
        }
        const properties = Object.keys(descriptors)
            .sort(compareStrings)
            .map((name) => describeProperty(name, descriptors[name]));
        const globalObjectAliases = findGlobalObjectAliases(
            value,
            descriptors,
            globalObject,
            errors,
            `realm-${index}`,
        );
        return {
            id: `realm-${index}`,
            sources,
            isPluginGlobal: value === globalObject,
            globalObjectAliases,
            identifierProperties: properties
                .filter((property) => property.isEnvironmentIdentifier)
                .map((property) => property.name),
            properties,
        };
    });

    return {
        schemaVersion: 2,
        privacy: {
            includesGlobalValues: false,
            includesVaultPaths: false,
            includesFileNames: false,
            includesNoteContent: false,
        },
        runtime: runtimeMetadata(globalObject),
        candidates: {
            globalBindings: Array.from(
                new Set(realms.flatMap((realm) => realm.identifierProperties)),
            ).sort(compareStrings),
            globalObjects: Array.from(
                new Set(realms.flatMap((realm) => realm.globalObjectAliases)),
            ).sort(compareStrings),
        },
        obsidianModuleBindings: probeModuleBindings(
            moduleExports,
            globalObject,
            errors,
        ),
        realms,
        errors: errors.sort(compareStrings),
    };
}

function probeModuleBindings(moduleExports, globalObject, errors) {
    const available = [];
    const unavailable = [];
    const evaluator = safeRead(globalObject, "eval", errors, "global.eval");
    if (typeof evaluator !== "function") {
        errors.push("global eval is unavailable; module bindings were not probed");
        return { available, unavailable };
    }

    for (const name of Object.keys(moduleExports).sort(compareStrings)) {
        if (!isEnvironmentIdentifier(name)) continue;
        try {
            const value = evaluator(name);
            available.push({
                name,
                location: Object.prototype.hasOwnProperty.call(globalObject, name)
                    ? "property"
                    : "lexical",
                sameAsModuleExport: value === moduleExports[name],
                valueType: typeof value,
            });
        } catch (error) {
            if (error && error.name === "ReferenceError") {
                unavailable.push(name);
            } else {
                errors.push(`binding ${name}: ${errorMessage(error)}`);
            }
        }
    }
    return { available, unavailable };
}

function addRealm(candidates, source, value) {
    if (!isObject(value)) return;
    const existing = candidates.find((candidate) => candidate.value === value);
    if (existing) {
        existing.sources.push(source);
        existing.sources.sort(compareStrings);
    } else {
        candidates.push({ sources: [source], value });
    }
}

function describeProperty(name, descriptor) {
    const isData = Object.prototype.hasOwnProperty.call(descriptor, "value");
    const description = {
        name,
        isEnvironmentIdentifier: isEnvironmentIdentifier(name),
        descriptor: isData ? "data" : "accessor",
        enumerable: Boolean(descriptor.enumerable),
        configurable: Boolean(descriptor.configurable),
    };
    if (isData) {
        description.writable = Boolean(descriptor.writable);
        description.valueType = typeof descriptor.value;
        try {
            description.valueTag = Object.prototype.toString.call(descriptor.value);
        } catch (error) {
            description.valueTag = "[unavailable]";
        }
    } else {
        description.hasGetter = typeof descriptor.get === "function";
        description.hasSetter = typeof descriptor.set === "function";
    }
    return description;
}

function findGlobalObjectAliases(realm, descriptors, pluginGlobal, errors, source) {
    const aliases = [];
    for (const [name, descriptor] of Object.entries(descriptors)) {
        if (
            isEnvironmentIdentifier(name)
            && Object.prototype.hasOwnProperty.call(descriptor, "value")
            && isWindowLike(descriptor.value)
        ) {
            aliases.push(name);
        }
    }
    for (const name of WELL_KNOWN_GLOBAL_OBJECT_NAMES) {
        if (isWindowLike(safeRead(realm, name, errors, `${source}.${name}`))) {
            aliases.push(name);
        }
    }
    if (realm === pluginGlobal) aliases.push("globalThis");
    return Array.from(new Set(aliases)).sort(compareStrings);
}

function runtimeMetadata(globalObject) {
    const processObject = safeRead(globalObject, "process", [], "runtime.process");
    const navigatorObject = safeRead(globalObject, "navigator", [], "runtime.navigator");
    const versions = isObject(processObject?.versions) ? processObject.versions : {};
    const userAgent =
        typeof navigatorObject?.userAgent === "string" ? navigatorObject.userAgent : null;
    const match = typeof userAgent === "string"
        ? /(?:^|\s)obsidian\/([^\s]+)/i.exec(userAgent)
        : null;
    return {
        obsidianVersion: match ? match[1] : null,
        electronVersion: stringOrNull(versions.electron),
        chromiumVersion: stringOrNull(versions.chrome),
        nodeVersion: stringOrNull(versions.node),
        platform: stringOrNull(processObject?.platform),
        userAgent,
    };
}

function safeRead(object, name, errors, source) {
    if (!isObject(object)) return undefined;
    try {
        return object[name];
    } catch (error) {
        errors.push(`${source}: ${errorMessage(error)}`);
        return undefined;
    }
}

function isWindowLike(value) {
    if (!isObject(value)) return false;
    try {
        return value.window === value && value.self === value;
    } catch (error) {
        return false;
    }
}

function isObject(value) {
    return (typeof value === "object" && value !== null) || typeof value === "function";
}

function isEnvironmentIdentifier(name) {
    return /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name);
}

function stringOrNull(value) {
    return typeof value === "string" ? value : null;
}

function errorMessage(error) {
    return error instanceof Error ? error.message : String(error);
}

function compareStrings(left, right) {
    return left.localeCompare(right, "en");
}

class GlobalReportModal extends Modal {
    constructor(app, report) {
        super(app);
        this.report = report;
    }

    onOpen() {
        this.titleEl.setText("Glass Lint global report");
        this.contentEl.createEl("p", {
            text: "This report contains global names and type metadata, but no global values, vault paths, filenames, or note content.",
        });

        const output = this.contentEl.createEl("textarea");
        output.value = this.report;
        output.readOnly = true;
        output.rows = 24;
        output.style.width = "100%";
        output.style.fontFamily = "var(--font-monospace)";
        output.addEventListener("focus", () => output.select());

        const copyButton = this.contentEl.createEl("button", {
            text: "Copy report",
            cls: "mod-cta",
        });
        copyButton.addEventListener("click", async () => {
            try {
                await navigator.clipboard.writeText(this.report);
                new Notice("Glass Lint global report copied");
            } catch (error) {
                output.focus();
                new Notice("Could not access the clipboard; press Ctrl/Cmd+C to copy");
            }
        });
    }

    onClose() {
        this.contentEl.empty();
    }
}

module.exports = class GlassLintGlobalProbePlugin extends Plugin {
    async onload() {
        this.lastReport = null;
        this.addCommand({
            id: "collect",
            name: "Collect plugin runtime globals",
            callback: () => this.collectAndShow(),
        });
    }

    collectAndShow() {
        const report = collectObsidianGlobals({
            app: this.app,
            globalObject: globalThis,
            moduleExports: obsidianApi,
        });
        this.lastReport = JSON.stringify(report, null, 2);
        console.log("Glass Lint global report", report);
        new GlobalReportModal(this.app, this.lastReport).open();
        return this.lastReport;
    }
};
