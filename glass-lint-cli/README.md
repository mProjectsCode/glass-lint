# glass-lint-cli

`glass-lint-cli` owns the `glass-lint` command. It selects a rule catalog,
loads configuration, runs single-file or project analysis, renders the result,
and maps outcomes to process exit statuses.

## Commands

```sh
cargo run -p glass-lint-cli --bin glass-lint -- rules
cargo run -p glass-lint-cli --bin glass-lint -- check path/to/project
cargo run -p glass-lint-cli --bin glass-lint -- snippet path/to/source.js
```

- `rules` lists metadata for the selected provider and profile.
- `check` analyzes an entry file, directory, or explicit `tsconfig.json` as one
  bounded project, including admitted internal imports.
- `snippet` analyzes exactly one source file without cross-file linking.

## Configuration

Pass a TOML or JSON file with `--config PATH`, or inline JSON with
`--config-json JSON`. Without either option, the command looks for
`glass-lint.toml` or `glass-lint.json` in the current directory.

Configuration is versioned and rejects unknown fields:

```toml
version = 2

[core]
rules = ["obsidian:network.request"]

[cli]
provider = "obsidian"
profile = "recommended"
fail_on = "warning"
output = "pretty"
verbosity = "quiet"
color = true
pretty_max_width = 120
show_evidence_source = true

[cli.project]
max_bytes = 8388608
max_project_bytes = 536870912
max_visited_entries = 250000
max_timeout_ms = 300000
```

The `core.rules` field selects exact rule IDs. When omitted, the chosen profile
is preserved; an empty list disables all rules. Project budgets are nested
under `[cli.project]` and are passed directly to `glass-lint-project`.

The default provider is `obsidian`, which runs both `js:*` and `obsidian:*`
rules in the Obsidian host environment. The default profile is `heuristic`;
choose `recommended` for high-confidence rules only. The standalone `js`
provider runs only `js:*` rules.

## Output and exit status

Pretty output is the default. Findings are grouped by rule, and evidence is
sorted by file and source location. Set `show_evidence_source = false` to keep
evidence locations and messages while omitting source excerpts and carets.

Set `output = "json"` for the versioned structured report. JSON is always
uncolored. Pretty output and tracing use color by default; set `color = false`
for plain text. Reports go to stdout, while operational errors and telemetry go
to stderr.

The process exits with:

- `0` when analysis succeeds and no finding reaches `fail_on`;
- `1` when a finding reaches the threshold, parsing fails, or project analysis
  is partial; and
- `2` for invalid arguments, invalid configuration, or operational errors.

`fail_on` accepts `info`, `warning`, `error`, and `never`; its default is
`error`.

See [ARCHITECTURE.md](ARCHITECTURE.md) for command-layer boundaries.
