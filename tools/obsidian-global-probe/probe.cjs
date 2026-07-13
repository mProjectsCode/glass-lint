"use strict";

const SCHEMA_VERSION = 1;
const WELL_KNOWN_GLOBAL_OBJECT_NAMES = [
    "activeWindow",
    "global",
    "globalThis",
    "self",
    "window",
];

function collectObsidianGlobals({ app, globalObject = globalThis } = {}) {
    const errors = [];
    const realmCandidates = [];

    addRealmCandidate(realmCandidates, "plugin-global", globalObject);
    for (const name of WELL_KNOWN_GLOBAL_OBJECT_NAMES) {
        const value = safelyRead(globalObject, name, errors, `global.${name}`);
        if (isWindowLike(value)) {
            addRealmCandidate(realmCandidates, `global.${name}`, value);
        }
    }

    collectWorkspaceRealms(app, realmCandidates, errors);

    const realms = realmCandidates.map((candidate, index) =>
        inspectRealm(candidate, index, globalObject, errors),
    );
    const globalBindings = Array.from(
        new Set(realms.flatMap((realm) => realm.identifierProperties)),
    ).sort(compareStrings);
    const globalObjects = Array.from(
        new Set(realms.flatMap((realm) => realm.globalObjectAliases)),
    ).sort(compareStrings);

    return {
        schemaVersion: SCHEMA_VERSION,
        privacy: {
            includesGlobalValues: false,
            includesVaultPaths: false,
            includesFileNames: false,
            includesNoteContent: false,
        },
        runtime: collectRuntimeMetadata(app, globalObject),
        candidates: {
            globalBindings,
            globalObjects,
        },
        realms,
        errors: errors.sort(compareStrings),
    };
}

function collectWorkspaceRealms(app, candidates, errors) {
    const workspace = app && app.workspace;
    if (!workspace || typeof workspace.iterateAllLeaves !== "function") {
        errors.push("app.workspace.iterateAllLeaves is unavailable");
        return;
    }

    let leafIndex = 0;
    try {
        workspace.iterateAllLeaves((leaf) => {
            const source = `workspace-leaf-${leafIndex}`;
            leafIndex += 1;
            try {
                const container = leaf && leaf.view && leaf.view.containerEl;
                const realm = container && container.ownerDocument
                    ? container.ownerDocument.defaultView
                    : undefined;
                if (isWindowLike(realm)) {
                    addRealmCandidate(candidates, source, realm);
                }
            } catch (error) {
                errors.push(`${source}: ${errorMessage(error)}`);
            }
        });
    } catch (error) {
        errors.push(`iterateAllLeaves: ${errorMessage(error)}`);
    }
}

function addRealmCandidate(candidates, source, value) {
    if (!isObject(value)) {
        return;
    }
    const existing = candidates.find((candidate) => candidate.value === value);
    if (existing) {
        existing.sources.push(source);
        existing.sources.sort(compareStrings);
        return;
    }
    candidates.push({ sources: [source], value });
}

function inspectRealm(candidate, index, pluginGlobal, errors) {
    const descriptors = safelyGetDescriptors(
        candidate.value,
        errors,
        `realm-${index}`,
    );
    const properties = Object.keys(descriptors)
        .sort(compareStrings)
        .map((name) => describeProperty(name, descriptors[name]));
    const identifierProperties = properties
        .filter((property) => property.isEnvironmentIdentifier)
        .map((property) => property.name);
    const globalObjectAliases = findGlobalObjectAliases(
        candidate.value,
        descriptors,
        pluginGlobal,
        errors,
        `realm-${index}`,
    );

    return {
        id: `realm-${index}`,
        sources: candidate.sources,
        isPluginGlobal: candidate.value === pluginGlobal,
        globalObjectAliases,
        identifierProperties,
        properties,
    };
}

function safelyGetDescriptors(value, errors, source) {
    try {
        return Object.getOwnPropertyDescriptors(value);
    } catch (error) {
        errors.push(`${source} descriptors: ${errorMessage(error)}`);
        return {};
    }
}

function describeProperty(name, descriptor) {
    const description = {
        name,
        isEnvironmentIdentifier: isEnvironmentIdentifier(name),
        descriptor: Object.prototype.hasOwnProperty.call(descriptor, "value")
            ? "data"
            : "accessor",
        enumerable: Boolean(descriptor.enumerable),
        configurable: Boolean(descriptor.configurable),
    };

    if (description.descriptor === "data") {
        description.writable = Boolean(descriptor.writable);
        description.valueType = safeTypeof(descriptor.value);
        description.valueTag = safeTag(descriptor.value);
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
        const value = safelyRead(realm, name, errors, `${source}.${name}`);
        if (isWindowLike(value)) {
            aliases.push(name);
        }
    }

    if (realm === pluginGlobal) {
        aliases.push("globalThis");
    }
    return Array.from(new Set(aliases)).sort(compareStrings);
}

function collectRuntimeMetadata(app, globalObject) {
    const processObject = safelyRead(globalObject, "process", [], "runtime.process");
    const versions = isObject(processObject) && isObject(processObject.versions)
        ? processObject.versions
        : {};
    const navigatorObject = safelyRead(globalObject, "navigator", [], "runtime.navigator");
    const userAgent =
        navigatorObject && typeof navigatorObject.userAgent === "string"
            ? navigatorObject.userAgent
            : null;
    const platform = typeof processObject?.platform === "string"
        ? processObject.platform
        : null;

    return {
        obsidianVersion:
            typeof globalObject?.obsidian?.version === "string"
                ? globalObject.obsidian.version
                : typeof app?.appVersion === "string"
                  ? app.appVersion
                  : obsidianVersionFromUserAgent(userAgent),
        electronVersion: stringOrNull(versions.electron),
        chromiumVersion: stringOrNull(versions.chrome),
        nodeVersion: stringOrNull(versions.node),
        platform,
        userAgent,
    };
}

function obsidianVersionFromUserAgent(userAgent) {
    if (typeof userAgent !== "string") {
        return null;
    }
    const match = /(?:^|\s)obsidian\/([^\s]+)/i.exec(userAgent);
    return match ? match[1] : null;
}

function safelyRead(object, name, errors, source) {
    if (!isObject(object)) {
        return undefined;
    }
    try {
        return object[name];
    } catch (error) {
        errors.push(`${source}: ${errorMessage(error)}`);
        return undefined;
    }
}

function isWindowLike(value) {
    if (!isObject(value)) {
        return false;
    }
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

function safeTypeof(value) {
    try {
        return typeof value;
    } catch (error) {
        return "unknown";
    }
}

function safeTag(value) {
    try {
        return Object.prototype.toString.call(value);
    } catch (error) {
        return "[unavailable]";
    }
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

module.exports = {
    collectObsidianGlobals,
    isEnvironmentIdentifier,
};
