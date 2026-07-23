// @case description shadowed, similar, dynamic, and reassigned request APIs
// @tool glass-lint rules=obsidian:network.request
function request() {}
// @expect-no-error glass-lint rule=obsidian:network.request
request("https://example.com");
import { request as localRequest } from "not-obsidian";
// @expect-no-error glass-lint rule=obsidian:network.request
localRequest("/local");

import * as obsidianNamespace from "obsidian";
function shadowedNamespace(obsidianNamespace) {
    // @expect-no-error glass-lint rule=obsidian:network.request
    obsidianNamespace.requestUrl("not-network");
}

// A local loader cannot provide module provenance.
function require(name) { return {}; }
// @expect-no-error glass-lint rule=obsidian:network.request
require('obsidian').request('/shadowed-loader');

const moduleName = 'obsidian';
// @expect-no-error glass-lint rule=obsidian:network.request
require(moduleName).requestUrl('/dynamic-module');

// A module export alias no longer matches after reassignment.
import { requestUrl } from 'obsidian';
let mutable = requestUrl;
mutable = localRequest;
// @expect-no-error glass-lint rule=obsidian:network.request
mutable('/reassigned');

const fallback = chooseFallback(require("obsidian"));
// @expect-no-error glass-lint rule=obsidian:network.request
fallback.requestUrl("https://example.com");
function localRequestUrl(url) { return `local:${url}`; }
// @expect-no-error glass-lint rule=obsidian:network.request
localRequestUrl("not-network");
