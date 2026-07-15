# glass-lint-harness

`glass-lint-harness` is the reusable conformance runner and profiler for Glass
Lint. It loads annotated JavaScript and TypeScript snippets, runs built-in or
external tool adapters, verifies structured findings, renders reports, and
profiles folders of supported JavaScript and TypeScript runtime files.

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
and must return one `AdapterResponse` on standard output using the current
`ADAPTER_PROTOCOL_VERSION`. The executable starts a new adapter process for
each case, which isolates tool-global state. Requests include the case language
(`javascript` or `typescript`); an adapter that does not support TypeScript may
reject those requests explicitly.

## Commands

The CLI front end supports:

- `verify PATH` — check assertions and return a failing status on mismatch;
- `report PATH --format markdown|json` — render suite results;
- `compare PATH` — run registered adapters and write a comparison report; and
- `profile --path PATH ...` — measure lint time across supported JavaScript and
  TypeScript runtime files.

Register external tools with a global `--adapter NAME=COMMAND` option:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.ts \
  verify tests/e2e
```

For case directives, diagnostic assertions, and test placement, see
[TESTING.md](../TESTING.md). Profiling options are documented in
[CONTRIBUTING.md](../CONTRIBUTING.md).
