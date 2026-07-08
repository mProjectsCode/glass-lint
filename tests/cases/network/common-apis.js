// @case description Ported old classifier case: common browser and Obsidian network APIs
// @tool glass-lint rules=obsidian:network.browser,obsidian:network.obsidian

import { requestUrl } from "obsidian"; // @expect-error glass-lint rule=obsidian:network.obsidian message_id=detected line=6
fetch("https://example.com"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
requestUrl("https://example.com");
navigator.sendBeacon("https://example.com", "{}"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
new XMLHttpRequest(); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
new WebSocket("wss://example.com"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
new EventSource("https://example.com/events"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
