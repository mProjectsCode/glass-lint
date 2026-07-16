# glass-lint-harness-cli

`glass-lint-harness-cli` owns the `glass-lint-harness` command for verifying
conformance cases, rendering reports, comparing adapters, and profiling
analysis.

## Run conformance cases

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  verify tests/e2e

cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  report tests/e2e --format json
```

`verify PATH` prints a summary and fails when actual results differ from case
expectations. `report PATH` renders Markdown by default and also accepts
`--format json`.

Register an external tool with the global `--adapter NAME=COMMAND` option:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.ts \
  verify tests/e2e
```

`compare PATH` runs all registered adapters and writes
`reports/COMPARISON.md`.

## Profile analysis

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- profile \
  --path path/to/bundles \
  --provider obsidian \
  --profile recommended \
  --sample 20 \
  --seed 20260712
```

Repeat `--path` to combine roots. `--include` and `--exclude` filter discovered
paths; `--rule` selects exact rule IDs. `--warm-up`, `--repeat`, and
`--workers` control execution. Add `--project` to profile one bounded
filesystem project per path and report discovery, reads, local analysis,
resolution, and linking/matching separately.

See [TESTING.md](../TESTING.md) for case authoring and
[CONTRIBUTING.md](../CONTRIBUTING.md) for profiling guidance.
