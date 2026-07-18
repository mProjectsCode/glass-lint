// @case description app-scoped local and secret storage operations
// @tool glass-lint rules=obsidian:storage.app-data
// @expect-error glass-lint rule=obsidian:storage.app-data message_id=detected
app.loadLocalStorage();
// @expect-error glass-lint rule=obsidian:storage.app-data message_id=detected
app.saveLocalStorage(settings);
// @expect-error glass-lint rule=obsidian:storage.app-data message_id=detected
app.secretStorage.getSecret("token");
// @expect-error glass-lint rule=obsidian:storage.app-data message_id=detected
app.secretStorage.setSecret("token", value);
// @expect-error glass-lint rule=obsidian:storage.app-data message_id=detected
app.secretStorage.listSecrets();

const secretStorage = this.app.secretStorage;
// @expect-error glass-lint rule=obsidian:storage.app-data message_id=detected
secretStorage.getSecret("alias");
