// @case description positive fixture for obsidian:ui.notice
// @tool glass-lint rules=obsidian:ui.notice
// @expect-error glass-lint rule=obsidian:ui.notice message_id=detected
new Notice('x');
// second independent example

// @expect-error glass-lint rule=obsidian:ui.notice message_id=detected
new Notice("second");
// Migrated: interface/classes-and-settings.js
import { Notice as notice } from "obsidian";

// @expect-error glass-lint rule=obsidian:ui.notice message_id=detected
const showNotice = () => new notice("done");
