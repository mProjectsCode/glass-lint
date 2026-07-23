// @case description ESM, CommonJS, namespace, and aliased request APIs
// @tool glass-lint rules=obsidian:network.request
// The active window may be the main window, so it shares the configured globals.
// @expect-error glass-lint rule=obsidian:network.request
activeWindow.requestUrl("https://example.com/active-window");

// @expect-error glass-lint rule=obsidian:network.request
globalThis.request("https://example.com/global");
const injectedRequest = window.requestUrl;
// @expect-error glass-lint rule=obsidian:network.request
injectedRequest("https://example.com/global-alias");

import { request } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request
request("https://example.com");

import { requestUrl as secondRequest } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request
secondRequest("/second");
const requestAlias = request;
// @expect-error glass-lint rule=obsidian:network.request
requestAlias("/aliased");

import { requestUrl } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request
requestUrl("https://example.com");

const obsidian = require("obsidian");
// @expect-error glass-lint rule=obsidian:network.request
obsidian.requestUrl("https://example.com");
const { requestUrl: destructuredRequest } = require("obsidian");
// @expect-error glass-lint rule=obsidian:network.request
destructuredRequest("https://example.com");

import { requestUrl as renamedRequest, request } from "obsidian";
import * as namespaceRequest from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request
renamedRequest("https://example.com");
// @expect-error glass-lint rule=obsidian:network.request
request("https://example.com");
const namespaceSend = namespaceRequest.request;
// @expect-error glass-lint rule=obsidian:network.request
namespaceSend("https://example.com");

// An alias is valid before reassignment.
let mutableRequest = request;
// @expect-error glass-lint rule=obsidian:network.request
mutableRequest('/before-reassignment');
mutableRequest = localRequest;
