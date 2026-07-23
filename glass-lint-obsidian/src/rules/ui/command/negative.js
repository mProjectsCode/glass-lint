// @case description other receivers, aliases, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:ui.command
import { Plugin } from "obsidian";
class TestPlugin extends Plugin {
  run() {
// @expect-no-error glass-lint rule=obsidian:ui.command
plugin.addCommand(command);

// Proven member extraction is supported; this is an expected finding.
const add = this.addCommand;
// @expect-error glass-lint rule=obsidian:ui.command
add(command);
// @expect-no-error glass-lint rule=obsidian:ui.command
this[dynamicProperty](command);
// @expect-no-error glass-lint rule=obsidian:ui.command
this.addCommands(command);
  }
}
