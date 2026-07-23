// @case description rooted resource calls and literal URL markers
// @tool glass-lint rules=obsidian:vault.resource-url

// @expect-error glass-lint rule=obsidian:vault.resource-url
app.vault.getResourcePath(file);
// @expect-error glass-lint rule=obsidian:vault.resource-url
app.vault.adapter.getResourcePath(file);
// @expect-error glass-lint rule=obsidian:vault.resource-url
this.app.vault.getResourcePath(otherFile);
// @expect-error glass-lint rule=obsidian:vault.resource-url
this.app.vault.adapter.getResourcePath(otherFile);

const vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.resource-url
vault.getResourcePath(file);
const adapter = app.vault.adapter;
// @expect-error glass-lint rule=obsidian:vault.resource-url
adapter.getResourcePath(file);

// @expect-error glass-lint rule=obsidian:vault.resource-url
const callbackUrl = "obsidian://open?vault=demo";
// @expect-error glass-lint rule=obsidian:vault.resource-url
const templateCallbackUrl = `obsidian://open?vault=${vault}`;
