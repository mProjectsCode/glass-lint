// @case description Compact local functions do not impersonate global or Obsidian APIs
// @tool glass-lint rules=js:network.request,obsidian:network.request
// @tool eslint-obsidianmd config=recommended

function fetch(r){return r}fetch("not-network"); // @expect-no-error glass-lint rule=js:network.request message_id=detected
function requestUrl(r){return r}requestUrl("not-network"); // @expect-no-error glass-lint rule=obsidian:network.request message_id=detected
