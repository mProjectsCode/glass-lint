// @case description Local classes and unused imports do not count as Obsidian API usage
// @tool glass-lint rules=obsidian:ui.modal,obsidian:ui.settings-tab,obsidian:markdown.link
// @tool eslint-obsidianmd config=recommended

import { Notice as ImportedNotice, Setting as ImportedSetting, MarkdownView as ImportedMarkdownView } from "obsidian";

class Notice {}
class Setting {}
class MarkdownView {}
new Notice("local");
new Setting(container);
const view = new MarkdownView();

console.log(ImportedNotice, ImportedSetting, ImportedMarkdownView);
