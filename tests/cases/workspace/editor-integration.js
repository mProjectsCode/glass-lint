// @case description Editor callbacks and workspace events are detected
// @tool glass-lint rules=obsidian:ui.command
// @tool eslint-obsidianmd config=recommended

this.addCommand({ id: "edit", editorCallback(editor) {} }); // @expect-error glass-lint rule=obsidian:ui.command message_id=detected line=any column=any
this.registerEvent(this.app.workspace.on("file-menu", menu => {}));
