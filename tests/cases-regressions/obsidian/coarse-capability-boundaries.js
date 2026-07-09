// @case description Ported classifier regression: reviewed coarse capabilities stay scoped to actual API use
// @tool glass-lint rules=obsidian:electron.desktop,obsidian:electron.ipc_shell,obsidian:ui.file_dialog,obsidian:metadata.extraction,obsidian:vault.enumerate

import { clipboard } from "electron"; // @expect-error glass-lint rule=obsidian:electron.desktop message_id=detected

const input = document.createElement("input");
input.type = "text"; // @expect-no-error glass-lint rule=obsidian:ui.file_dialog message_id=detected

const localModel = { tags: [], links: [], embeds: [] };
localModel.tags; // @expect-no-error glass-lint rule=obsidian:metadata.extraction message_id=detected

this.app.vault.getRoot(); // @expect-no-error glass-lint rule=obsidian:vault.enumerate message_id=detected
const adapter = this.app.vault;
