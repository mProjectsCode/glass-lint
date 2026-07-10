// @case description negative fixture for obsidian:network.request
// @tool glass-lint rules=obsidian:network.request
// @expect-no-error glass-lint rule=obsidian:network.request message_id=detected
function request() {}
request("https://example.com");
import { request as localRequest } from "not-obsidian";

// @expect-no-error glass-lint rule=obsidian:network.request message_id=detected
localRequest("/local");
// Migrated: network/namespace-import-shadowing.js and reject-require-consumers.js
import * as obsidianNamespace from "obsidian";
function shadowedNamespace(obsidianNamespace) {

// @expect-no-error glass-lint rule=obsidian:network.request message_id=detected
  obsidianNamespace.requestUrl("not-network");
}
const fallback = chooseFallback(require("obsidian"));
fallback.requestUrl("https://example.com");
// Migrated: network/compact-local-lookalikes.js and network/local-lookalikes.js
function localRequestUrl(url) { return `local:${url}`; }
localRequestUrl("not-network");
