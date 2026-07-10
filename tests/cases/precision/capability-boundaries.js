// @case description Coarse capabilities stay scoped to actual API use
// @tool glass-lint rules=js:electron.module,js:electron.ipc,js:browser.file-dialog,obsidian:metadata.extract,obsidian:vault.enumerate
// @tool eslint-obsidianmd config=recommended

import { clipboard } from "electron"; // @expect-error glass-lint rule=js:electron.module message_id=detected

const input = document.createElement("input");
input.type = "text"; // @expect-no-error glass-lint rule=js:browser.file-dialog message_id=detected

const localModel = { tags: [], links: [], embeds: [] };
localModel.tags; // @expect-no-error glass-lint rule=obsidian:metadata.extract message_id=detected

this.app.vault.getRoot(); // @expect-error glass-lint rule=obsidian:vault.enumerate message_id=detected
const adapter = this.app.vault;
