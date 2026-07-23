// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:storage.plugin-data-write
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write
plugin.saveData(data);

const save = this.saveData;
// @expect-error glass-lint rule=obsidian:storage.plugin-data-write
save(data);
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write
this[dynamicProperty](data);
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-write
this.saveDatas(data);
  }
}
