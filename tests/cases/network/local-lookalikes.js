// @case description Ported old classifier cases: local network lookalikes do not count
// @tool glass-lint rules=obsidian:network.obsidian,obsidian:network.browser

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
