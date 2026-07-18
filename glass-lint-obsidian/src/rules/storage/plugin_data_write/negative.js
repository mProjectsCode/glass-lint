// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:storage.plugin-data-write
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
plugin.saveData(data);

const save = this.saveData;
// @expect-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
save(data);
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
this[dynamicProperty](data);
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write message_id=detected
this.saveDatas(data);
  }
}
