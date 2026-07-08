// @case description Ported old classifier cases: local shadowing only hides calls in its lexical scope
// @tool glass-lint rules=obsidian:network.browser,obsidian:network.obsidian

import { requestUrl } from "obsidian"; // @expect-error glass-lint rule=obsidian:network.obsidian message_id=detected line=6

requestUrl("https://example.com");
function localOnly(requestUrl) {
  requestUrl("not-network");
}

function localFetchOnly(fetch) {
  fetch("not-network");
}
function networkCall() {
  fetch("https://example.com"); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected
}
