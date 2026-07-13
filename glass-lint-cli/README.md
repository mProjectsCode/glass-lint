# glass-lint-cli

`glass-lint-cli` provides the workspace's two command-line programs:

- `glass-lint` analyzes JavaScript files and directories and prints JSON
  reports.
- `glass-lint-harness` verifies annotated snippets, renders reports, compares
  adapters, and profiles folders.

The package is a front end. Reusable analysis belongs in `glass-lint-core` and
provider crates; reusable case execution belongs in `glass-lint-harness`.

## `glass-lint`

```sh
cargo run -p glass-lint-cli --bin glass-lint -- rules
cargo run -p glass-lint-cli --bin glass-lint -- check path/to/main.js
cargo run -p glass-lint-cli --bin glass-lint -- snippet path/to/snippet.js
```

Use the global `--provider js|obsidian` option to choose a catalog. Both
`check` and `snippet` accept repeated `--rule` options, a
`--profile recommended|heuristic`, and `--fail-on info|warning|error|never`.
`check` recursively discovers `.js` files in directories and enforces its
configurable `--max-bytes` filesystem limit. `snippet` analyzes the specified
path without that front-end size check; the core 8 MiB parser limit still
applies.

Output is a JSON array of `(filename, report)` pairs. Exit status is `0` for a
successful run below the failure threshold, `1` for matching findings or parse
diagnostics, and `2` for usage or operational errors.

## `glass-lint-harness`

```sh
cargo run -p glass-lint-cli --bin glass-lint-harness -- verify tests/e2e
cargo run -p glass-lint-cli --bin glass-lint-harness -- \
  report tests/e2e --format json
```

See the [`glass-lint-harness` README](../glass-lint-harness/) for its command
overview and [TESTING.md](../TESTING.md) for authoring cases.
