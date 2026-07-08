// @case description Ported old classifier cases: Obsidian request imports, aliases, namespaces, and bundler wrappers
// @tool glass-lint rules=obsidian:network.obsidian

import { requestUrl as renamedRequestUrl, request } from "obsidian";
import * as obsidian from "obsidian";

renamedRequestUrl("https://example.com");
request("https://example.com");
(0, request)("https://example.com");
obsidian.requestUrl("https://example.com"); // @expect-error glass-lint rule=obsidian:network.obsidian message_id=detected count=4
(0, obsidian.request)("https://example.com"); // @expect-error glass-lint rule=obsidian:network.obsidian message_id=detected

const send = renamedRequestUrl;
send("https://example.com");

const sendFromNamespace = obsidian.request; // @expect-error glass-lint rule=obsidian:network.obsidian message_id=detected
sendFromNamespace("https://example.com");
