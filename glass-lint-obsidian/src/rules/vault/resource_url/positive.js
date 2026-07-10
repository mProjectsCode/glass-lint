// @case description positive fixture for obsidian:vault.resource-url
// @tool glass-lint rules=obsidian:vault.resource-url

// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
app.vault.getResourcePath(file);

// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
app.vault.getResourcePath(otherFile);

const v = app.vault;
// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
v1.getResourcePath(file);

// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const callbackUrl = "obsidian://open?vault=demo";

// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const templateCallbackUrl = `obsidian://open?vault=${vault}`;
