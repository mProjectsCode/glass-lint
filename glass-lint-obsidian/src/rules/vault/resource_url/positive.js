// @case description rooted resource calls and literal URL markers
// @tool glass-lint rules=obsidian:vault.resource-url

// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
app.vault.getResourcePath(file);
// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
app.vault.adapter.getResourcePath(file);
// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
this.app.vault.getResourcePath(otherFile);
// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
this.app.vault.adapter.getResourcePath(otherFile);

const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
vault.getResourcePath(file);
const adapter = app.vault.adapter;
// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
adapter.getResourcePath(file);

// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const callbackUrl = "obsidian://open?vault=demo";
// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const templateCallbackUrl = `obsidian://open?vault=${vault}`;
