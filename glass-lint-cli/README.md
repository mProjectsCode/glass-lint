# glass-lint-cli

`glass-lint-cli` provides the workspace's `glass-lint` command:

- `glass-lint` analyzes JavaScript or TypeScript files and directories and prints reports.

The harness executable is provided by the separate `glass-lint-harness-cli`
package.

The package is a front end. Reusable analysis belongs in `glass-lint-core` and
provider crates; reusable case execution belongs in `glass-lint-harness`.

## `glass-lint`

```sh
cargo run -p glass-lint-cli --bin glass-lint -- rules
cargo run -p glass-lint-cli --bin glass-lint -- check path/to/main.js
cargo run -p glass-lint-cli --bin glass-lint -- snippet path/to/snippet.js
```

Use `--config PATH` or `--config-json JSON`; without either, configuration is
discovered from the current directory. `check` recursively discovers supported
JavaScript and TypeScript runtime files (`.js`, `.cjs`, `.mjs`, `.ts`, `.cts`,
and `.mts`); declaration files are excluded. `snippet` requires a file. Policy
lives in the versioned `[core]` and `[cli]` sections.

The default Obsidian provider runs both generic `js:*` rules and
Obsidian-specific `obsidian:*` rules in one analysis pass using the Obsidian
host environment. The `heuristic` profile enables all rules; set 
`cli.profile = "recommended"` to only include high-confidence discovery rules.

Pretty output is the default; JSON uses a named versioned envelope. Results are
on stdout, while operational errors and telemetry are on stderr. Exit status is
`0` below the threshold, `1` for findings/parse diagnostics meeting it, and
`2` for operational errors.

Pretty output and tracing use color by default. Set `cli.color = false` in
`glass-lint.toml` (or `"color": false` in inline JSON) for plain output. JSON
output is always uncolored.

## `glass-lint-harness`

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- verify tests/e2e
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  report tests/e2e --format json
```

See the [`glass-lint-harness` README](../glass-lint-harness/) for its command
overview and [TESTING.md](../TESTING.md) for authoring cases.
