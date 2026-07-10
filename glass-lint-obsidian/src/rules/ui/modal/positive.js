// @case description positive fixture for obsidian:ui.modal
// @tool glass-lint rules=obsidian:ui.modal
// @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
new Modal(app);

// Migrated: interface/classes-and-settings.js
import { Modal as LegacyModal } from "obsidian";
class LegacyExampleModal extends LegacyModal {} // @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
// second independent example
// @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
new Modal(app);
