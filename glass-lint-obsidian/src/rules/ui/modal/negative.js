// @case description local, shadowed, reassigned, and dynamic Modal lookalikes
// @tool glass-lint rules=obsidian:ui.modal
// @expect-no-error glass-lint rule=obsidian:ui.modal message_id=detected
class LocalModal {}
new LocalModal();

// @expect-no-error glass-lint rule=obsidian:ui.modal message_id=detected
function shadowed(Modal) {
    new Modal();
}

const localModule = 'obsidian';
require(localModule).Modal;

import { Modal as ImportedModal } from 'obsidian';
let Alias = ImportedModal;
Alias = LocalModal;
// @expect-no-error glass-lint rule=obsidian:ui.modal message_id=detected
new Alias();

class DialogModal {}
new DialogModal();
