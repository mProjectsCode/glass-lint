"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const {
    collectObsidianGlobals,
    isEnvironmentIdentifier,
} = require("./probe.cjs");

const fakeWindow = {};
fakeWindow.window = fakeWindow;
fakeWindow.self = fakeWindow;
fakeWindow.globalThis = fakeWindow;
fakeWindow.activeWindow = fakeWindow;
Object.defineProperty(fakeWindow, "lazyGlobal", {
    configurable: true,
    enumerable: false,
    get() {
        throw new Error("the inventory must not invoke arbitrary getters");
    },
});
fakeWindow.fetch = function fetch() {};
fakeWindow["not-a-binding"] = true;

const app = {
    appVersion: "test-version",
    workspace: {
        iterateAllLeaves(callback) {
            callback({
                view: {
                    containerEl: {
                        ownerDocument: { defaultView: fakeWindow },
                    },
                },
            });
        },
    },
};

const report = collectObsidianGlobals({ app, globalObject: fakeWindow });
assert.equal(report.schemaVersion, 1);
assert.deepEqual(report.candidates.globalObjects, [
    "activeWindow",
    "globalThis",
    "self",
    "window",
]);
assert(report.candidates.globalBindings.includes("fetch"));
assert(report.candidates.globalBindings.includes("lazyGlobal"));
assert(!report.candidates.globalBindings.includes("not-a-binding"));
assert.equal(report.realms.length, 1);
assert.deepEqual(report.realms[0].sources, [
    "global.activeWindow",
    "global.globalThis",
    "global.self",
    "global.window",
    "plugin-global",
    "workspace-leaf-0",
]);
assert.equal(report.errors.length, 0);
assert(isEnvironmentIdentifier("activeWindow"));
assert(!isEnvironmentIdentifier("window.fetch"));

const pluginEntry = fs.readFileSync(path.join(__dirname, "main.js"), "utf8");
assert(!/require\(["']\.\.?\//.test(pluginEntry));

console.log("obsidian global probe tests passed");
