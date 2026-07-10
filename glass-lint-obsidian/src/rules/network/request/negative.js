// @case description negative fixture for obsidian:network.request
// @tool glass-lint rules=obsidian:network.request
// @expect-no-error glass-lint rule=obsidian:network.request message_id=detected
function request() {}
request("https://example.com");
import { request as localRequest } from "not-obsidian";
// @expect-no-error glass-lint rule=obsidian:network.request message_id=detected
localRequest("/local");

// Migrated: network/namespace-import-shadowing.js and reject-require-consumers.js
import * as legacyObsidianNamespace from "obsidian";
function legacyShadowedNamespace(legacyObsidianNamespace) {
  legacyObsidianNamespace.requestUrl("not-network"); // @expect-no-error glass-lint rule=obsidian:network.request message_id=detected
}
const legacyFallback = chooseFallback(require("obsidian"));
legacyFallback.requestUrl("https://example.com");

// Migrated: network/compact-local-lookalikes.js and network/local-lookalikes.js
function legacyLocalRequestUrl(url) { return `local:${url}`; }
legacyLocalRequestUrl("not-network");
