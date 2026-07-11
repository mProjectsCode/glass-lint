// @case description lookalike receivers, dynamic URLs, and reassignment
// @tool glass-lint rules=obsidian:vault.resource-url

// @expect-no-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const localApp = { vault: { getResourcePath() {} } };
localApp.vault.getResourcePath(file);
// @expect-no-error glass-lint rule=obsidian:vault.resource-url message_id=detected
function shadowed(app) {
  app.vault.getResourcePath(file);
}
shadowed({ vault: { getResourcePath() {} } });
// @expect-no-error glass-lint rule=obsidian:vault.resource-url message_id=detected
app.vault[method](file);
// @expect-no-error glass-lint rule=obsidian:vault.resource-url message_id=detected
app.vault.getAttachmentPath(file);
// @expect-no-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const splitUrl = "obsidian:" + "//open?vault=demo";
// @expect-no-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const dynamicUrl = scheme + "//open?vault=demo";
// @expect-no-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const otherScheme = "https://example.test";
// @expect-no-error glass-lint rule=obsidian:vault.resource-url message_id=detected
const wrongCase = "Obsidian://open?vault=demo";

let vault = app.vault;
// @expect-error glass-lint rule=obsidian:vault.resource-url message_id=detected
vault.getResourcePath(file);
vault = localVault;
// @expect-no-error glass-lint rule=obsidian:vault.resource-url message_id=detected
vault.getResourcePath(file);
