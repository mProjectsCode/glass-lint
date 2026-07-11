// @case description configured markers and static marker fragments
// @tool glass-lint rules=obsidian:plugins.dataview
// @expect-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const dataview = 'dataview';
// @expect-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const dataviewApi = 'dataviewapi';
// @expect-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const dataCoreDashed = 'data-core';
// @expect-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const dataCore = 'datacore';

// @expect-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const embeddedMarker = 'using dataview in a string';
// Static template fragments are indexed, but interpolated values are not.
// @expect-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const template = `datacore: ${runtimeValue}`;
