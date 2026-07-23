// @case description local, shadowed, reassigned, and dynamic Modal lookalikes
// @tool glass-lint rules=obsidian:ui.modal
// Obsidian does not inject Modal into the plugin realm.
// @expect-no-error glass-lint rule=obsidian:ui.modal
new Modal(app);
const UnboundModalAlias = Modal;
// @expect-no-error glass-lint rule=obsidian:ui.modal
new UnboundModalAlias(app);

// @expect-no-error glass-lint rule=obsidian:ui.modal
class LocalModal {}
new LocalModal();

// @expect-no-error glass-lint rule=obsidian:ui.modal
function shadowed(Modal) {
    new Modal();
}

const localModule = 'obsidian';
require(localModule).Modal;

import { Modal as ImportedModal } from 'obsidian';
let Alias = ImportedModal;
Alias = LocalModal;
// @expect-no-error glass-lint rule=obsidian:ui.modal
new Alias();

class DialogModal {}
new DialogModal();
