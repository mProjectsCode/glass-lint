// @case description both configured open calls through rooted aliases and static properties
// @tool glass-lint rules=obsidian:workspace.open

// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.openLinkText(name, source);
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.getLeaf.openFile(file);
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.getLeaf().openFile(file);
const leaf = app.workspace.getLeaf();
const leafAlias = leaf;
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
leafAlias.openFile(aliasedFile);

// `this.app`, workspace aliases, and static computed names retain provenance.
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
this.app.workspace["openLinkText"]("computed", source);
const workspace = app.workspace;
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
workspace.openLinkText("alias", source);
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
workspace.getLeaf.openFile(otherFile);
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.getLeafById("leaf").openFile(file);
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.getLeftLeaf(leaf).openFile(file);
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.getRightLeaf(leaf).openFile(file);
// @expect-error glass-lint rule=obsidian:workspace.open message_id=detected
app.workspace.ensureSideLeaf("left").openFile(file);
