// @case description Obsidian request imports, aliases, namespaces, and bundler wrappers preserve provenance
// @tool glass-lint rules=obsidian:network.request
// @tool eslint-obsidianmd config=recommended

import { requestUrl as renamedRequestUrl, request } from "obsidian";
import * as obsidian from "obsidian";

renamedRequestUrl("https://example.com");
request("https://example.com");
(0, request)("https://example.com");
obsidian.requestUrl("https://example.com"); // @expect-error glass-lint rule=obsidian:network.request message_id=detected count=7 line=any
(0, obsidian.request)("https://example.com");

const send = renamedRequestUrl;
send("https://example.com");

const sendFromNamespace = obsidian.request;
sendFromNamespace("https://example.com");
