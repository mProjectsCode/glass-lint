// @case description Compact local functions do not impersonate global or Obsidian APIs
// @tool glass-lint rules=obsidian:network.browser,obsidian:network.obsidian

function fetch(r){return r}fetch("not-network"); // @expect-no-error glass-lint rule=obsidian:network.browser message_id=detected
function requestUrl(r){return r}requestUrl("not-network"); // @expect-no-error glass-lint rule=obsidian:network.obsidian message_id=detected
