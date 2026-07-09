// @case description Module and global provenance respect lexical shadowing
// @tool glass-lint rules=obsidian:network.browser,obsidian:network.obsidian

import { requestUrl } from "obsidian";

requestUrl("https://example.com"); // @expect-error glass-lint rule=obsidian:network.obsidian message_id=detected
function localRequest(requestUrl) {
  requestUrl("not-network"); // @expect-no-error glass-lint rule=obsidian:network.obsidian message_id=detected
}

function localFetch(fetch) {
  fetch("not-network"); // @expect-no-error glass-lint rule=obsidian:network.browser message_id=detected
}
fetch("https://example.com"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
