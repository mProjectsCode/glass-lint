// @case description ESM, CommonJS, namespace, and aliased request APIs
// @tool glass-lint rules=obsidian:network.request
import { request } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
request("https://example.com");

import { requestUrl as secondRequest } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
secondRequest("/second");
const requestAlias = request;
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
requestAlias("/aliased");

import { requestUrl } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
requestUrl("https://example.com");

const obsidian = require("obsidian");
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
obsidian.requestUrl("https://example.com");
const { requestUrl: destructuredRequest } = require("obsidian");
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
destructuredRequest("https://example.com");

import { requestUrl as renamedRequest, request } from "obsidian";
import * as namespaceRequest from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
renamedRequest("https://example.com");
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
request("https://example.com");
const namespaceSend = namespaceRequest.request;
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
namespaceSend("https://example.com");

// An alias is valid before reassignment.
let mutableRequest = request;
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
mutableRequest('/before-reassignment');
mutableRequest = localRequest;
