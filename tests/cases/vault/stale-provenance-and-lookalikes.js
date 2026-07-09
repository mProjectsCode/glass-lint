// @case description Stale provenance and local API lookalikes are rejected
// @tool glass-lint rules=obsidian:vault.read,obsidian:vault.write

let vault = this.app.vault;
vault = localStore;
vault.read(file);

function localOnly() {
  const app = {
    vault: {
      read() {},
      modify() {}
    }
  };
  app.vault.read(file);
  app.vault.modify(file, text);
}
