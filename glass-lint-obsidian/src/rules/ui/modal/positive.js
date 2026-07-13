// @case description module, namespace, and CommonJS Modal forms
// @tool glass-lint rules=obsidian:ui.modal
import { Modal as ImportedModal } from "obsidian";
// @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
new ImportedModal(app);
// @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
class ExampleModal extends ImportedModal {}

import * as obsidian from "obsidian";
// @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
new obsidian.Modal(app);

const { Modal: CommonJsModal } = require('obsidian');
// @expect-error glass-lint rule=obsidian:ui.modal message_id=detected
new CommonJsModal(app);
