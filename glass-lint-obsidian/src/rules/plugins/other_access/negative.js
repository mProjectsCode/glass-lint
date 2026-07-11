// @case description shadowing, reassignment, dynamic properties, and lookalikes
// @tool glass-lint rules=obsidian:plugins.other-access
// @expect-no-error glass-lint rule=obsidian:plugins.other-access message_id=detected
function localApp(app) {
    app.plugins.getPlugin('local');
}

// @expect-no-error glass-lint rule=obsidian:plugins.other-access message_id=detected
let plugins = otherPlugins;
plugins = otherPlugins;
plugins.getPlugin('reassigned');

// @expect-no-error glass-lint rule=obsidian:plugins.other-access message_id=detected
app.plugins[property];
// @expect-no-error glass-lint rule=obsidian:plugins.other-access message_id=detected
app.plugins.getOtherPlugin('lookalike');

// @expect-no-error glass-lint rule=obsidian:plugins.other-access message_id=detected
const localAppObject = { plugins: { getPlugin() {} } };
localAppObject.plugins.getPlugin('local');
