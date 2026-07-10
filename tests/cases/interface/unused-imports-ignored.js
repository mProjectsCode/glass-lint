// @case description Unused Obsidian class imports are not class usage
// @tool glass-lint rules=obsidian:ui.modal,obsidian:ui.settings-tab,obsidian:markdown.link
// @tool eslint-obsidianmd config=recommended

import { Notice, Setting, MarkdownView } from "obsidian";
console.log("imports only");
