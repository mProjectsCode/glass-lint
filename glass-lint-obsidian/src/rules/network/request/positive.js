// @case description positive fixture for obsidian:network.request
// @tool glass-lint rules=obsidian:network.request
import { request } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
request("https://example.com");
// second independent example
import { requestUrl as secondRequest } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
secondRequest("/second");
const requestAlias = request;
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
requestAlias("/aliased");

// Migrated: network/common-apis.js
import { requestUrl as legacyRequestUrl } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
legacyRequestUrl("https://example.com");

// Migrated: network/commonjs-provenance.js
const legacyObsidian = require("obsidian");
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
legacyObsidian.requestUrl("https://example.com");
const { requestUrl: legacyDestructuredRequest } = require("obsidian");
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
legacyDestructuredRequest("https://example.com");

// Migrated: network/obsidian-import-provenance.js
import { requestUrl as legacyRenamedRequest, request as legacyRequest } from "obsidian";
import * as legacyNamespaceRequest from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
legacyRenamedRequest("https://example.com");
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
legacyRequest("https://example.com");
const legacyNamespaceSend = legacyNamespaceRequest.request;
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
legacyNamespaceSend("https://example.com");
