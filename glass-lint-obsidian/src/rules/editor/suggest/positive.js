// @case description direct and statically-computed suggestion registration
// @tool glass-lint rules=obsidian:editor.suggest
// @expect-error glass-lint rule=obsidian:editor.suggest message_id=detected
this.registerEditorSuggest(s);
// @expect-error glass-lint rule=obsidian:editor.suggest message_id=detected
this['registerEditorSuggest'](secondSuggest);

// The syntactic heuristic does not establish the receiver's provider type.
function unrelatedReceiver() {
    // @expect-error glass-lint rule=obsidian:editor.suggest message_id=detected
    this.registerEditorSuggest(unrelatedSuggest);
}
