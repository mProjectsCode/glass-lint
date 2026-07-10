// @case description positive fixture for obsidian:ui.modal
// @tool glass-lint rules=obsidian:ui.modal
// @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
new Modal(app);
// Migrated: interface/classes-and-settings.js
import { Modal as modal } from "obsidian";
class exampleModal extends modal {} // @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
// second independent example

// @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
new Modal(app);
