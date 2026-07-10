// @case description positive fixture for obsidian:ui.command
// @tool glass-lint rules=obsidian:ui.command
// @expect-error glass-lint rule=obsidian:ui.command message_id=detected
this.addCommand({id:'x'});
// second independent example

// @expect-error glass-lint rule=obsidian:ui.command message_id=detected
this.addCommand({ id: "second" });
// Migrated: workspace/editor-integration.js and plain-commands-ignored.js

// @expect-error glass-lint rule=obsidian:ui.command message_id=detected
this.addCommand({ id: "legacy-edit", editorCallback(editor) {} });

// @expect-error glass-lint rule=obsidian:ui.command message_id=detected
this.addCommand({ id: "legacy-plain", callback() {} });
