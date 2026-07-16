# ESLint Obsidian adapter

This adapter runs `eslint-plugin-obsidianmd` and translates its diagnostics
into the Glass Lint harness protocol. It exists for conformance comparisons;
normal workspace builds do not need it.

Install dependencies from the adapter directory:

```sh
cd adapters/eslint-obsidianmd
bun install
cd ../..
```

Then register the Bun-powered executable with the harness:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.ts \
  verify tests/e2e
```

Each invocation reads one protocol request from stdin, analyzes its source with
`ESLint.lintText`, and writes one protocol response to stdout. A new process is
used for every case, which isolates the plugin's manifest cache. Expectations
remain compatible with RuleTester rule IDs and message IDs.

Run `make compare` to generate the repository comparison report. See
[`TESTING.md`](../../TESTING.md) for harness directives and external adapter
behavior.
