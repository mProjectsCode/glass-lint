// @case description local, dynamic, aliased, and reassigned editor lookalikes
// @tool glass-lint rules=obsidian:editor.content
const localEditor = {
  getValue() {},
  setValue() {},
  getRange() {},
};
// @expect-no-error glass-lint rule=obsidian:editor.content message_id=detected
localEditor.getValue();

function unrelatedReceiver() {
  // @expect-no-error glass-lint rule=obsidian:editor.content message_id=detected
  this.getValue();
}

const method = getMethodName();
// @expect-no-error glass-lint rule=obsidian:editor.content message_id=detected
localEditor[method]();

let editor = localEditor;
editor = anotherEditor;
// @expect-no-error glass-lint rule=obsidian:editor.content message_id=detected
editor.setValue(value);
