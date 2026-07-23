// @case description proven Obsidian Editor content reads and mutations
// @tool glass-lint rules=obsidian:editor.content
import { Editor } from "obsidian";
class TestEditor extends Editor {
  run() {
    // @expect-error glass-lint rule=obsidian:editor.content
    this.getValue();
    // @expect-error glass-lint rule=obsidian:editor.content
    this.setValue(value);
    // @expect-error glass-lint rule=obsidian:editor.content
    this.getLine(0);
    // @expect-error glass-lint rule=obsidian:editor.content
    this.setLine(0, value);
    // @expect-error glass-lint rule=obsidian:editor.content
    this.getRange(from, to);
    // @expect-error glass-lint rule=obsidian:editor.content
    this.replaceRange(value, from, to);
    // @expect-error glass-lint rule=obsidian:editor.content
    this.getSelection();
    // @expect-error glass-lint rule=obsidian:editor.content
    this.replaceSelection(value);
    // @expect-error glass-lint rule=obsidian:editor.content
    this.getCursor();
    // @expect-error glass-lint rule=obsidian:editor.content
    this.setCursor(cursor);
    // @expect-error glass-lint rule=obsidian:editor.content
    this.setSelection(from, to);
    // @expect-error glass-lint rule=obsidian:editor.content
    this.setSelections(selections);
    // @expect-error glass-lint rule=obsidian:editor.content
    this["getValue"]();
  }
}
