// @case description shadowed, reassigned, dynamic, and lookalike constructors
// @tool glass-lint rules=obsidian:ui.notice
// @expect-no-error glass-lint rule=obsidian:ui.notice
function shadowed(Notice) {
  new Notice();
}
shadowed(class LocalNotice {});

// @expect-no-error glass-lint rule=obsidian:ui.notice
class Noticeable {}
new Noticeable();

import { Notice as ImportedNotice } from "obsidian";
class LocalNotice {}
let reassigned = ImportedNotice;
reassigned = LocalNotice;
// @expect-no-error glass-lint rule=obsidian:ui.notice
new reassigned();

const moduleName = "obsidian";
// @expect-no-error glass-lint rule=obsidian:ui.notice
new require(moduleName).Notice();
