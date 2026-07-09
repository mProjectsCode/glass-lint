// @case description Network string matchers ignore comments, identifiers, and version-like literals
// @tool glass-lint rules=obsidian:network.ai_provider,obsidian:vault.obsidian_config,obsidian:network.private

// api.openai.com should not classify a provider by itself.
/* .obsidian/plugins/example */
const endpoint = getEndpoint();
const obsidianProtocol = buildProtocol();
const apiOpenaiCom = getHost();
const version = "10.4.2";
const range = "172.20.1";
const text = "192.168.";
