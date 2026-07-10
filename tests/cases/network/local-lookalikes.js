// @case description Local network lookalikes do not report network capability use
// @tool glass-lint rules=obsidian:network.request,js:network.request
// @tool eslint-obsidianmd config=recommended

function requestUrl(url) {
  return `local:${url}`;
}
requestUrl("not-network");

function localFetch(value) {
  function fetch(value) {
    return value;
  }
  fetch("not-network");
}

function minifiedRequestUrl(r) {
  return r;
}
minifiedRequestUrl("not-network");
