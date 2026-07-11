// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:lifecycle.events
// @expect-no-error glass-lint rule=obsidian:lifecycle.events message_id=detected
plugin.registerEvent(handler);

const register = this.registerEvent;
// @expect-no-error glass-lint rule=obsidian:lifecycle.events message_id=detected
register(handler);

// @expect-no-error glass-lint rule=obsidian:lifecycle.events message_id=detected
this[dynamicMethod](handler);

// @expect-no-error glass-lint rule=obsidian:lifecycle.events message_id=detected
this.registerEventual(handler);

// @expect-no-error glass-lint rule=obsidian:lifecycle.events message_id=detected
this.registerDomEvents(element, handler);
