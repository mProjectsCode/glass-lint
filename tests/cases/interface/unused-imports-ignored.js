// @case description Unused Obsidian class imports are not class usage
// @tool glass-lint rules=obsidian:ui.modals_notices,obsidian:settings.ui,obsidian:editor.markdown_api

import { Notice, Setting, MarkdownView } from "obsidian";
console.log("imports only");
