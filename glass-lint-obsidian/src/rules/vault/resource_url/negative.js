// @case description lookalike receivers, dynamic URLs, and reassignment
// @tool glass-lint rules=obsidian:vault.resource-url

// @expect-no-error glass-lint rule=obsidian:vault.resource-url
const localApp = { vault: { getResourcePath() {} } };
localApp.vault.getResourcePath(file);
// @expect-no-error glass-lint rule=obsidian:vault.resource-url
function shadowed(app) {
  app.vault.getResourcePath(file);
}
shadowed({ vault: { getResourcePath() {} } });
// @expect-no-error glass-lint rule=obsidian:vault.resource-url
app.vault[method](file);
// @expect-no-error glass-lint rule=obsidian:vault.resource-url
app.vault.getAttachmentPath(file);
// @expect-no-error glass-lint rule=obsidian:vault.resource-url
const splitUrl = scheme + "//open?vault=demo";
// @expect-no-error glass-lint rule=obsidian:vault.resource-url
const dynamicUrl = scheme + "//open?vault=demo";
// @expect-no-error glass-lint rule=obsidian:vault.resource-url
const otherScheme = "https://example.test";
// @expect-no-error glass-lint rule=obsidian:vault.resource-url
const wrongCase = "Obsidian://open?vault=demo";

let reassignedVault = app.vault;
reassignedVault = localVault;
// @expect-no-error glass-lint rule=obsidian:vault.resource-url
reassignedVault.getResourcePath(file);
