# ESLint Obsidian adapter

Install the pinned dependencies with `bun install`, then register the Bun-powered adapter:

```sh
glass-lint-harness --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.ts verify tests/e2e
```

The harness starts a fresh process for each case, isolating the plugin's manifest cache. The adapter uses `ESLint.lintText` because structured diagnostics are the harness output; its expectation format remains compatible with RuleTester rule IDs and message IDs.
