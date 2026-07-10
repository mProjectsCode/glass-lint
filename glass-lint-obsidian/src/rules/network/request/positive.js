// @case description positive fixture for obsidian:network.request
// @tool glass-lint rules=obsidian:network.request
import { request } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
request("https://example.com");
// second independent example
import { requestUrl as secondRequest } from "obsidian";
// @expect-error glass-lint rule=obsidian:network.request message_id=detected
secondRequest("/second");
