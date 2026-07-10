// @case description negative fixture for obsidian:ui.modal
// @tool glass-lint rules=obsidian:ui.modal
// @expect-no-error glass-lint rule=obsidian:ui.modal message_id=detected
function localLookalike() { return null; }
localLookalike();

// @expect-no-error glass-lint rule=obsidian:ui.modal message_id=detected
class DialogModal {}
new DialogModal();
// Migrated: interface/local-classes-ignored.js and unused-imports-ignored.js
import { Notice as unusedNotice } from "obsidian";
class localModal {}
new localModal();
unusedNotice;
