// @case description dynamic values, concatenations, and unrelated markers
// @tool glass-lint rules=obsidian:plugins.dataview
// @expect-no-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const runtimeValue = pluginName;
const dynamic = `${runtimeValue}`;

// @expect-no-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const concatenated = 'data' + 'view';
// @expect-no-error glass-lint rule=obsidian:plugins.dataview message_id=detected
const unrelated = 'data-view';
