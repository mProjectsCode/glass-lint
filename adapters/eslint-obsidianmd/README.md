# ESLint Obsidian adapter

This external harness adapter runs `eslint-plugin-obsidianmd` and translates
its diagnostics into the Glass Lint harness protocol. It exists for
conformance comparisons; it is not required to build or run Glass Lint.

Install dependencies from the adapter directory:

```sh
cd adapters/eslint-obsidianmd
bun install
cd ../..
```

Then register the Bun-powered executable with the harness:

```sh
cargo run -p glass-lint-cli --bin glass-lint-harness -- \
  --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.ts \
  verify tests/e2e
```

The harness starts a fresh process for each case, isolating the plugin's
manifest cache. The adapter uses `ESLint.lintText` because the harness consumes
structured diagnostics. Expectations remain compatible with RuleTester rule
IDs and message IDs.

Run `make compare` to generate the repository comparison report. See
[`TESTING.md`](../../TESTING.md) for harness directives and external adapter
behavior.
