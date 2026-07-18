// @case description proven Obsidian Editor content reads and mutations
// @tool glass-lint rules=obsidian:editor.content
import { Editor } from "obsidian";
class TestEditor extends Editor {
  run() {
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.getValue();
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.setValue(value);
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.getLine(0);
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.setLine(0, value);
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.getRange(from, to);
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.replaceRange(value, from, to);
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.getSelection();
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.replaceSelection(value);
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.getCursor();
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.setCursor(cursor);
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.setSelection(from, to);
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this.setSelections(selections);
    // @expect-error glass-lint rule=obsidian:editor.content message_id=detected
    this["getValue"]();
  }
}
