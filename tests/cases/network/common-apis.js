// @case description Common browser and Obsidian network APIs are detected
// @tool glass-lint rules=obsidian:network.browser,obsidian:network.obsidian
// @tool eslint-obsidianmd config=recommended

import { requestUrl } from "obsidian";
fetch("https://example.com"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
requestUrl("https://example.com"); // @expect-error glass-lint rule=obsidian:network.obsidian message_id=detected
navigator.sendBeacon("https://example.com", "{}"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
new XMLHttpRequest(); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
new WebSocket("wss://example.com"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
new EventSource("https://example.com/events"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
