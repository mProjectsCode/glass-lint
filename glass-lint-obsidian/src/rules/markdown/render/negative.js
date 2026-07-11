// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:markdown.render
// @expect-no-error glass-lint rule=obsidian:markdown.render message_id=detected
renderer.render(app, text, el, '', ctx);

// @expect-no-error glass-lint rule=obsidian:markdown.render message_id=detected
MarkdownRenderer.renderMarkdown(source);

const render = MarkdownRenderer.render;
// @expect-no-error glass-lint rule=obsidian:markdown.render message_id=detected
render(app, text, el, '', ctx);

// @expect-no-error glass-lint rule=obsidian:markdown.render message_id=detected
MarkdownRenderer[dynamicMethod](app, text, el, '', ctx);
