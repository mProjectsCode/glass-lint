# glass-lint-harness-cli

`glass-lint-harness-cli` provides the `glass-lint-harness` command.

## Cases and reports

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  verify tests/e2e

cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  report tests/e2e --format json
```

`verify` fails on expectation mismatches. `report` renders Markdown by default
and also accepts JSON. Register an external tool globally:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  --adapter eslint-obsidianmd=adapters/eslint-obsidianmd/adapter.ts \
  compare tests/e2e
```

`compare` writes `reports/COMPARISON.md`.

## Profiling

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- profile \
  --path path/to/bundles \
  --provider obsidian \
  --profile recommended \
  --sample 20 \
  --seed 20260712
```

Repeat `--path` to combine roots. Use `--include` and `--exclude` for path
filters, `--rule` for exact rule IDs, and `--warm-up`, `--repeat`, and
`--workers` for execution policy.

The default mode loads sources before timing independent-file lint calls.
`--project` measures filesystem project loading through matching.
`--admitted-project` measures the explicit core source-admission path without
resolver answers and therefore may report typed partial outcomes.

Freeze a corpus selection for reproducible comparisons:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- profile \
  --path path/to/bundles \
  --sample 100 --seed 0 \
  --create-manifest path/to/profile-manifest.json \
  --root-label release-mainjs

cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- profile \
  --path path/to/bundles \
  --manifest path/to/profile-manifest.json \
  --warm-up 1 --repeat 3 --workers 1 --quiet
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for CLI boundaries and
[TESTING.md](../TESTING.md) for case authoring.
