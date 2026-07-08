// @case description Ported old classifier regression: local classes and unused imports should not count as Obsidian API usage
// @tool glass-lint rules=obsidian:ui.modals_notices,obsidian:settings.ui,obsidian:editor.markdown_api

import { Notice as ImportedNotice, Setting as ImportedSetting, MarkdownView as ImportedMarkdownView } from "obsidian";

class Notice {}
class Setting {}
class MarkdownView {}
new Notice("local");
new Setting(container);
const view = new MarkdownView();

console.log(ImportedNotice, ImportedSetting, ImportedMarkdownView);
