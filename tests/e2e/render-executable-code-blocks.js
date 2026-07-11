// @case description A plugin evaluates JavaScript code blocks and renders string results
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd config=default
// @expect-error glass-lint rule=obsidian:markdown.code-block-processor count=2 line=any
// @expect-error glass-lint rule=js:dynamic-code.eval count=2 line=any

import { Plugin } from "obsidian";

export default class ExecutableCodeBlocksPlugin extends Plugin {
    onload() {
        this.registerMarkdownCodeBlockProcessor(
            "run-js",
            (source, element) => {
                const run = new Function(`return (${source})`);
                this.renderResult(element, run());
            },
        );
        this.registerMarkdownCodeBlockProcessor(
            "run-js-async",
            async (source, element) => {
                const AsyncFunction = Object.getPrototypeOf(
                    async function () {},
                ).constructor;
                const run = new AsyncFunction(`return (${source})`);
                this.renderResult(element, await run());
            },
        );
    }

    renderResult(element, value) {
        element.empty();
        element.setText(String(value));
    }
}
