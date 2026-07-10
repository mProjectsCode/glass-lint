// @case description String literal markers are detected in literals and templates
// @tool glass-lint rules=obsidian:vault.resource-url,obsidian:vault.config-directory,js:network.service-indicator
// @tool eslint-obsidianmd config=recommended

const callback = "obsidian://open?vault=demo"; // @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const config = ".obsidian/plugins/example/data.json"; // @expect-error glass-lint rule=obsidian:vault.config-directory message_id=detected
const endpoint = "https://api.openai.com/v1/chat/completions"; // @expect-error glass-lint rule=js:network.service-indicator message_id=detected
const templatedCallback = `obsidian://open?vault=${vault}`; // @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const templatedEndpoint = `https://api.openai.com/v1/${resource}`; // @expect-error glass-lint rule=js:network.service-indicator message_id=detected
