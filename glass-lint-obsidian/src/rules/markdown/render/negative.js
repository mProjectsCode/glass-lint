// @case description receiver, alias, dynamic-property, and near-name exclusions
// @tool glass-lint rules=obsidian:markdown.render
// @expect-no-error glass-lint rule=obsidian:markdown.render
renderer.render(app, text, el, '', ctx);

function localRenderer(MarkdownRenderer) {
    // @expect-no-error glass-lint rule=obsidian:markdown.render
    MarkdownRenderer.render(app, text, el, '', ctx);
}

// @expect-no-error glass-lint rule=obsidian:markdown.render
MarkdownRenderer.renderMarkdown(source);

const render = MarkdownRenderer.render;
// @expect-no-error glass-lint rule=obsidian:markdown.render
render(app, text, el, '', ctx);

// @expect-no-error glass-lint rule=obsidian:markdown.render
MarkdownRenderer[dynamicMethod](app, text, el, '', ctx);
