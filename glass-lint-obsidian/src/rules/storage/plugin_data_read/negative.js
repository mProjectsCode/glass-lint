// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:storage.plugin-data-read
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-read
plugin.loadData();

const load = this.loadData;
// @expect-error glass-lint rule=obsidian:storage.plugin-data-read
load();
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-read
this[dynamicProperty]();
// @expect-no-error glass-lint rule=obsidian:storage.plugin-data-read
this.loadDatas();
  }
}
