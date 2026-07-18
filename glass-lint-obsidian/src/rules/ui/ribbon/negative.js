// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:ui.ribbon
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:ui.ribbon message_id=detected
plugin.addRibbonIcon("other");

const addRibbonIcon = this.addRibbonIcon;
// @expect-error glass-lint rule=obsidian:ui.ribbon message_id=detected
addRibbonIcon("alias");

// @expect-no-error glass-lint rule=obsidian:ui.ribbon message_id=detected
this[dynamicProperty]("dynamic");

// @expect-no-error glass-lint rule=obsidian:ui.ribbon message_id=detected
this.addRibbonIcons("near-name");
  }
}
