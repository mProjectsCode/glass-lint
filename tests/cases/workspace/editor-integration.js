// @case description Ported old classifier cases: editor callbacks and workspace events
// @tool glass-lint rules=obsidian:workspace.editor_commands

this.addCommand({ id: "edit", editorCallback(editor) {} }); // @expect-error glass-lint rule=obsidian:workspace.editor_commands message_id=detected
this.registerEvent(this.app.workspace.on("file-menu", menu => {})); // @expect-error glass-lint rule=obsidian:workspace.editor_commands message_id=detected
