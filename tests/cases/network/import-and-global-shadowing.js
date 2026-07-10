// @case description Module and global provenance respect lexical shadowing
// @tool glass-lint rules=js:network.request,obsidian:network.request
// @tool eslint-obsidianmd config=recommended

import { requestUrl } from "obsidian";

requestUrl("https://example.com"); // @expect-error glass-lint rule=obsidian:network.request message_id=detected
function localRequest(requestUrl) {
  requestUrl("not-network"); // @expect-no-error glass-lint rule=obsidian:network.request message_id=detected
}

function localFetch(fetch) {
  fetch("not-network"); // @expect-no-error glass-lint rule=js:network.request message_id=detected
}
fetch("https://example.com"); // @expect-error glass-lint rule=js:network.request message_id=detected
