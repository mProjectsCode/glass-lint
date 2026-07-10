// @case description Network string matchers ignore comments, identifiers, and version-like literals
// @tool glass-lint rules=js:network.service-indicator,obsidian:vault.config-directory,js:network.private-address
// @tool eslint-obsidianmd config=recommended

// api.openai.com should not classify a provider by itself.
/* .obsidian/plugins/example */
const endpoint = getEndpoint();
const obsidianProtocol = buildProtocol();
const apiOpenaiCom = getHost();
const version = "10.4.2";
const range = "172.20.1";
const text = "192.168.";
