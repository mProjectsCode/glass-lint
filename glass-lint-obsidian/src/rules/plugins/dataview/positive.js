// @case description positive fixture for obsidian:plugins.dataview
// @tool glass-lint rules=obsidian:plugins.dataview
// @expect-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const x='dataview';
// second independent example

// @expect-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const secondIntegration = "datacore";
