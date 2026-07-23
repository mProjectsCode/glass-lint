// @case description rooted getActiveFile calls through aliases and static properties
// @tool glass-lint rules=obsidian:workspace.active-file

// @expect-error glass-lint rule=obsidian:workspace.active-file
app.workspace.getActiveFile();

// `this.app`, receiver aliases, and static computed names retain provenance.
// @expect-error glass-lint rule=obsidian:workspace.active-file
this.app.workspace["getActiveFile"]();
const workspace = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.active-file
workspace.getActiveFile();
