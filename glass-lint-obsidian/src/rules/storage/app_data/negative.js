// @case description shadowed, dynamic, reassigned, and lookalike app storage
// @tool glass-lint rules=obsidian:storage.app-data
const localApp = {
  loadLocalStorage() {},
  saveLocalStorage() {},
  secretStorage: { getSecret() {} },
};
// @expect-no-error glass-lint rule=obsidian:storage.app-data
localApp.loadLocalStorage();
// @expect-no-error glass-lint rule=obsidian:storage.app-data
localApp.secretStorage.getSecret("local");

function shadowed(app) {
  // @expect-no-error glass-lint rule=obsidian:storage.app-data
  app.loadLocalStorage();
}
shadowed(localApp);

const property = getPropertyName();
// @expect-no-error glass-lint rule=obsidian:storage.app-data
app.secretStorage[property]();

let storage = app.secretStorage;
storage = localStorage;
// @expect-no-error glass-lint rule=obsidian:storage.app-data
storage.getSecret("reassigned");
