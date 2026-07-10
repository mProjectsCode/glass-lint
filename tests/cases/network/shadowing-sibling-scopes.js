// @case description Local shadowing hides calls only in its lexical scope
// @tool glass-lint rules=obsidian:network.browser,obsidian:network.obsidian
// @tool eslint-obsidianmd config=recommended

import { requestUrl } from "obsidian";

requestUrl("https://example.com"); // @expect-error glass-lint rule=obsidian:network.obsidian message_id=detected
function localOnly(requestUrl) {
  requestUrl("not-network");
}

function localFetchOnly(fetch) {
  fetch("not-network");
}
function networkCall() {
  fetch("https://example.com"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
}
