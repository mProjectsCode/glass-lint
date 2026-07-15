# glass-lint-harness

`glass-lint-harness` is the reusable conformance runner and profiler for Glass
Lint. It loads annotated JavaScript and TypeScript snippets or `case.toml`
project fixtures, runs built-in or external tool adapters, verifies structured
findings, renders reports, and profiles supported runtime sources.

Most users invoke it through the `glass-lint-harness` binary in
`glass-lint-cli`:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- verify tests/e2e
```

## Library surface

The crate exports:

- case loading and suite execution;
- built-in and external adapter types;
- suite, case, expectation, and adapter protocol models;
- Markdown, JSON, comparison, and failure reports; and
- deterministic file discovery and profiling summaries.

External adapters receive one serialized `AdapterRequest` on standard input
and must return one `AdapterResponse` on standard output using protocol version
3. Project requests contain a root, entries, language-tagged files, and
optional explicit resolution records. Adapters that only support one source
are skipped deterministically for project cases. The executable starts a new
adapter process for each case, which isolates tool-global state.

## Commands

The CLI front end supports:

- `verify PATH` — check assertions and return a failing status on mismatch;
- `report PATH --format markdown|json` — render suite results;
- `compare PATH` — run registered adapters and write a comparison report; and
- `profile --path PATH ...` — measure lint time across supported JavaScript and
  TypeScript runtime files;
- `profile --project --path PATH ...` — measure one bounded filesystem project
  per path and print discovery, read, parse/local, resolution, and
  link/matching phases plus operation counts.

Project fixtures use a `case.toml` beside their sources. Set
`filesystem = true` to exercise `glass-lint-project`; otherwise list explicit
`[[resolution]]` records to exercise the virtual core linker. Source comments
continue to hold `@tool` and diagnostic assertions.

Register external tools with a global `--adapter NAME=COMMAND` option:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.ts \
  verify tests/e2e
```

For case directives, diagnostic assertions, and test placement, see
[TESTING.md](../TESTING.md). Profiling options are documented in
[CONTRIBUTING.md](../CONTRIBUTING.md).
