# CLI usability implementation plan

## Goal and constraints

Turn `glass-lint` into a pleasant interactive command without weakening its
machine-readable interface or the existing crate boundaries. The lint result is
the command's product and must be written only to stdout. Operational errors,
progress, diagnostics about the run, and tracing/timing data must be written to
stderr. This is the conventional and composable split: `glass-lint ... >
report.json` remains valid even at high verbosity, while a caller can redirect
or suppress telemetry independently.

The work should preserve these repository invariants:

- `glass-lint-core` remains provider-neutral and must not read files, discover
  configuration, select an Obsidian/JavaScript profile, detect terminals, or
  install a tracing subscriber. It does own the reusable engine configuration
  type and the canonical tracing formatter/subscriber factory; front ends
  choose when and where to use them.
- `glass-lint-cli` owns filesystem discovery, configuration precedence,
  provider/profile selection, aggregate output, exit codes, and subscriber
  setup.
- `glass-lint-harness` remains the reusable harness library. Its executable
  moves to a dedicated front-end package rather than into the library.
- Lint findings, evidence, source locations, file traversal, and rendered file
  order remain deterministic. Timing and tracing metadata must not be added to
  the serialized `LintReport`, because it is nondeterministic run metadata.
- Source text and inline configuration must never be included in tracing
  fields.

## Target workspace layout

```text
glass-lint-core/          provider-neutral analysis, reports, pretty report renderer, spans/events
glass-lint-js/            JavaScript provider
glass-lint-obsidian/      Obsidian provider
glass-lint-cli/           only the glass-lint executable
glass-lint-harness/       reusable case runner, adapters, reports, and profiler
glass-lint-harness-cli/   only the glass-lint-harness executable
```

`glass-lint-cli` must no longer depend on `glass-lint-harness`. The new
`glass-lint-harness-cli` package depends on `glass-lint-harness`, `clap`,
`anyhow`, `serde_json`, and the tracing subscriber support used by the existing
binary. Move `glass-lint-cli/src/bin/glass-lint-harness.rs` without retaining a
compatibility binary in `glass-lint-cli`; this project permits clean breaking
layout changes. Update the workspace members, `Makefile`, architecture diagram,
testing/contributing commands, crate READMEs, and root README in the same
change.

## User-facing command and configuration contract

Keep the current commands and positional input shape for the first iteration:

```text
glass-lint [--config PATH | --config-json JSON] rules
glass-lint [--config PATH | --config-json JSON] check PATH
glass-lint [--config PATH | --config-json JSON] snippet PATH
```

`--config` and `--config-json` are global and mutually exclusive. All current
policy flags move into the configuration: `provider`, `profile`, `rules`,
`max_bytes`, and `fail_on`. Add `output`, `verbosity`, and bounded pretty-print
options. The only remaining command argument is the path, since it identifies
the invocation rather than a reusable policy. Do not retain duplicate
command-line overrides in this clean break; one source of configuration avoids
an undocumented merge model.

Compose the file from two ownership-aligned types:

- `glass_lint_core::CoreConfig` owns provider-neutral engine choices reusable
  by Rust API callers. Initially this is exact rule selection. It is the future
  home for genuine analysis budgets or engine switches, but not filesystem
  limits, provider profiles, presentation, logging, or exit policy.
- `glass_lint_cli::CliConfig` owns provider/profile selection, filesystem input
  limits, failure threshold, output settings, and verbosity.
- The CLI's top-level `Config` owns the file schema version and embeds
  `CoreConfig` and `CliConfig`. Both nested sections default when omitted. The
  outer version covers the composed on-disk schema; do not add a second nested
  version until `CoreConfig` is offered as an independently loaded file.

Use this strict, versioned TOML schema:

```toml
version = 1

[core]
# Omit rules to inherit the provider profile. If present, the list is exact;
# an explicitly empty list disables every rule.
rules = ["obsidian:network.request"]

[cli]
provider = "obsidian"      # "obsidian" | "js"
profile = "recommended"   # "recommended" | "heuristic"
max_bytes = 8388608
fail_on = "error"          # "info" | "warning" | "error" | "never"
output = "pretty"          # "pretty" | "json"
verbosity = "normal"       # "quiet" | "normal" | "verbose" | "trace"
color = true               # color human output and tracing
pretty_max_width = 160      # display columns, including excerpt decorations
```

Equivalent inline JSON:

```sh
glass-lint --config-json \
  '{"version":1,"cli":{"provider":"js","output":"json"}}' check main.js
```

Implementation details:

- Define `CoreConfig` and its provider-neutral enums in `glass-lint-core`.
  Derive Serde serialization/deserialization and `Default`, and expose a small
  validated, consuming application API such as
  `Linter::configured(self, &CoreConfig)`. Model rule selection as
  `Option<Vec<RuleId>>`: `None` keeps the base provider/profile selection,
  `Some([])` enables no rules, and `Some(ids)` enables exactly those IDs. Do
  not make provider profile names part of core.
- Define `Config`, `CliConfig`, and CLI policy enums in `glass-lint-cli`. Derive
  Serde serialization/deserialization, apply section/field defaults with
  `Default`, and use `deny_unknown_fields` on all config structs so misspelled
  policy is an actionable error rather than silently ignored.
- Require `version = 1` uniformly in discovered files, explicit files, and
  inline JSON. Deserialize through a small raw config type so missing and
  unsupported versions can receive distinct errors before field defaults are
  applied.
- Validate after deserialization: `max_bytes` is greater than zero and no
  greater than `glass_lint_core::MAX_SOURCE_BYTES`; `pretty_max_width` leaves
  enough room for a useful excerpt; and every explicit rule ID exists in the
  selected provider catalog. `CoreConfig` performs generic catalog membership
  validation, while the CLI adds the clearer provider-mismatch context.
  Unknown rules fail before reading input files.
- Align the default `max_bytes` with the core's current 8 MiB analysis limit.
  The existing 10 MiB CLI default promises a size that core cannot analyze.
  Apply the limit consistently to both `check` and `snippet`; `snippet` should
  require a file, while `check` may accept a file or recursively discover `.js`
  files.
- Serialize the documented core and CLI defaults in examples and test them
  directly so library behavior, CLI defaults, and documentation cannot drift.

For Rust API reuse, document the same path used by the CLI:

```rust
let base = glass_lint_obsidian::recommended_linter();
let linter = base.configured(&core_config)?;
let report = linter.lint(source, "main.js");
```

If consuming a full CLI config from Rust is not desired, callers can
deserialize `CoreConfig` as a nested value in their own application format.
Core does not perform TOML/JSON I/O; it owns the Serde data model and validation
so every front end gets identical rule-selection semantics.

Configuration resolution is deterministic and based only on the process's
current working directory, never on the analyzed path and never by walking up
parent directories:

1. If `--config-json` is present, parse it as JSON and do no file discovery.
2. Else if `--config PATH` is present, resolve a relative path against the cwd,
   choose TOML or JSON from its `.toml`/`.json` extension, and do no discovery.
3. Else look in the cwd for `glass-lint.toml` and `glass-lint.json`.
4. If exactly one exists, load it. If both exist, return an operational/config
   error naming both files instead of applying surprising precedence.
5. If neither exists, use `Config::default()`.

Config parsing, I/O, schema-version, unknown-field, and validation errors go to
stderr and exit with status 2. Never log the raw inline JSON because it may
later contain sensitive values.

## Human-readable and JSON output

### Core pretty printer

Add a focused `glass-lint-core::report` module with a public, allocation-light
renderer, preferably a `PrettyReport<'a>` value implementing `Display`. Its
constructor accepts `&LintReport`, filename, and source text. Keeping source
outside `LintReport` avoids bloating or breaking the serialized report schema.
Accept a `PrettyOptions` value owned by core; the CLI translates
`CliConfig::pretty_max_width` into it. This keeps rendering behavior reusable
without making terminal or CLI policy part of core. The renderer must:

- group findings by rule across all files, then render each located evidence
  occurrence in file and source order with a copyable `path:line:column`
  prefix;
- render a one-line source excerpt and caret/range underline when the line is
  available, handling Unicode display columns, tabs, empty/multiline ranges,
  and out-of-range defensive cases without panicking;
- strictly bound every source excerpt and underline to
  `PrettyOptions::max_width`. For long or minified input, choose a window
  centered around the primary range, reserve width for line-number gutters and
  ellipses, preserve Unicode boundaries, expand or account for tabs
  consistently, and add leading/trailing `...` when text was omitted. Keep the
  highlighted range visible; if the range itself is wider than the budget,
  show its beginning and indicate that it continues. Diagnostic headers remain
  lossless even when a path or message itself is unusually long;
- render bounded, source-ordered evidence below its rule without repeating an
  occurrence across location-based findings;
- render parse diagnostics with their code and optional location;
- keep terminal detection in front ends, while the provider-neutral pretty
  renderer may apply an explicitly resolved color setting;
- use `std::fmt` plus a focused Unicode display-width helper if necessary; and
- have golden-style unit tests for findings, evidence, parse failures, Unicode,
  tabs, multiline ranges, missing ranges, empty reports, exact-width boundary
  cases, very long minified lines, findings near both ends of a long line, and
  exact trailing newline behavior.

A syntax-aware truncation mode is a stretch goal, not a prerequisite. It may
prefer nearby safe lexical boundaries such as whitespace, commas, semicolons,
or balanced delimiter edges while keeping the finding centered. It must not
reparse the file, introduce a second AST traversal, retain a duplicate parser,
or make rendering nondeterministic. If genuinely syntax-aware boundaries
cannot be carried cheaply from the existing parse/report assembly, ship the
range-centered bounded window first rather than violating parse-once.

The pretty renderer is a generic presentation of core report types, not an
Obsidian-specific policy surface. Re-export only the renderer from `lib.rs` and
document it in `glass-lint-core/README.md`.

### CLI aggregation

Make `pretty` the default interactive output. For each sorted file, write the
core pretty report to stdout, with exactly one separator between non-empty file
reports, followed by a deterministic aggregate summary such as `N file(s), F
finding(s), D parse diagnostic(s)`. Empty successful runs still print the
summary. Do not print progress text on stdout.

Replace the undocumented JSON tuple array with a named CLI envelope:

```json
{
  "schema_version": 1,
  "files": [{"path": "main.js", "report": {}}],
  "summary": {"files": 1, "findings": 0, "parse_diagnostics": 0}
}
```

The embedded core report retains its own `schema_version`. The CLI envelope
gets a separate version because it has separate compatibility ownership. JSON
is pretty-printed, contains no timing/log fields, ends in one newline, and is
the only stdout content when `output = "json"`. Add a CLI-owned pretty table
for `rules`; its JSON form remains structured rule metadata rather than the
lint envelope.

Centralize stdout writes behind output functions taking an injected `Write` so
unit tests can assert exact bytes and broken-pipe handling can be explicit. A
closed stdout pipe should terminate cleanly rather than print a second error to
stdout. All `anyhow`/usage errors remain on stderr.

Exit status remains:

- 0: successful analysis below `fail_on`;
- 1: at least one parse diagnostic or finding meeting `fail_on`; and
- 2: argument, configuration, discovery, file I/O, serialization, or other
  operational failure.

Output is written before returning status 1, so CI users receive the complete
report for a lint failure.

## Tracing, verbosity, and timing

Add `tracing` to `glass-lint-core`. Core emits spans/events and also owns a
small public `telemetry` module containing the canonical event formatter and
subscriber/layer builder. Put `tracing-subscriber` behind a core `telemetry`
feature so embedders that provide their own subscriber do not pay for the
formatting stack; both CLI packages enable that feature explicitly. This keeps
timestamps, levels, targets, fields, span timing, and line layout identical in
`glass-lint`, `glass-lint-harness`, and future tools.

Core must not install a global subscriber automatically. Each executable calls
the core setup/builder explicitly near startup, chooses its verbosity, and
supplies an explicit stderr writer. Prefer a composable layer/builder API over
an unconditional `init()` so the harness CLI can combine the shared formatter
with any harness-specific progress layer and tests can use scoped subscribers.
Provide a convenience `try_init` only if it returns installation errors to the
caller and still accepts the writer/filter configuration. A core-owned
`TelemetryConfig` can express tracing levels and span-close events; each front
end translates its own verbosity enum into that generic type.

Map configuration verbosity to filters as follows:

| Config | Filter | Intended content |
|---|---|---|
| `quiet` | WARN | warnings only |
| `normal` | INFO | invocation start/completion and aggregate counts |
| `verbose` | DEBUG | discovery, config source, per-file stages and timings |
| `trace` | TRACE | bounded internal stage detail useful to engine developers |

Operational errors must still be printed on stderr even if filtering would
hide tracing events. Do not make `RUST_LOG` part of the initial public contract;
the config remains the predictable verbosity authority.

Instrument coarse, meaningful boundaries rather than individual AST nodes:

- CLI: configuration resolved (source kind/path, never contents), linter built,
  discovery started/completed, file inspected/read, file lint started/completed,
  output rendered, and command completed.
- Core `Linter::lint`: filename, source byte count, selected rule count, final
  finding/diagnostic counts.
- Core stages: parse, semantic scope/resolution setup, fact collection/index
  construction, matcher query/classification, and report assembly/sort.
- Bounded-analysis fallbacks or invalid fact-stream paths: warning/debug events
  with a stable reason field, but no source fragments.

Use tracing spans as the timing markers and configure span close events at
`verbose`/`trace`, which yields elapsed/busy time on stderr without changing
library return types. Add explicit elapsed fields only where a span cannot
represent a boundary cleanly. Use stable target names such as
`glass_lint::cli`, `glass_lint::lint`, `glass_lint::parse`,
`glass_lint::semantic`, and `glass_lint::matching` so filters and captured tests
remain understandable. Avoid per-rule/per-fact INFO events and record counts,
not collections, to keep logs bounded.

The shared formatter should emit ordinary tracing fields rather than special
`progress` strings. Human progress behavior remains a front-end decision, but
when displayed its event layout should pass through the same core formatter.
Formatting tests live in core; each binary gets an integration test proving it
installs the shared layer on stderr and does not contaminate stdout.

Instrumentation must not add an AST traversal or make matcher-independent fact
construction depend on the selected rule set. Where the current
`classify_compiled_api_usage` boundary combines semantic construction and
matching, introduce internal stage functions/spans only; do not duplicate its
semantic model or expose analysis internals publicly.

## Implementation sequence

### 1. Establish executable boundaries

- Add `glass-lint-harness-cli` and move the harness binary intact.
- Remove `glass-lint-harness` and harness-only dependencies from
  `glass-lint-cli`.
- Update workspace metadata and all `cargo run`/build/profile/Makefile paths.
- Run `cargo check --workspace` and the existing harness suites before making
  behavior changes, proving the package move did not alter the harness.

### 2. Introduce and test configuration

- Refactor the one-file lint binary into small CLI-owned modules for arguments,
  config loading/validation, discovery, output, and command execution.
- Add `CoreConfig` and its catalog-validation/application tests in core. Add
  TOML/JSON dependencies to `glass-lint-cli` and implement the exact resolution
  algorithm above.
- Build the provider/profile base linter from validated `CliConfig`, then apply
  validated `CoreConfig` through the public core API.
- Add unit tests for both section defaults, both formats, inline JSON, every
  precedence branch, cwd-only discovery, dual-file ambiguity, unknown
  keys/version/enums, invalid sizes/pretty widths/rules, omitted versus
  explicitly empty rules, provider mismatch, and relative explicit paths.

### 3. Add core pretty formatting and CLI output modes

- Implement and re-export the bounded core renderer with focused unit tests,
  including minified-source truncation. Treat syntax-aware window selection as
  a follow-up only after the deterministic range-centered renderer is complete.
- Add CLI aggregation, the versioned JSON envelope, the rules table, summaries,
  and write/error handling.
- Add black-box CLI tests using the built executable and temporary cwd fixtures
  to assert stdout, stderr, and exit status separately for clean input,
  findings, parse errors, invalid config, explicit config, inline config,
  directory ordering, pretty output, and JSON output.

### 4. Add tracing and timings

- Instrument core coarse stages and add scoped-subscriber tests that capture
  events/spans to verify field presence without asserting wall-clock durations.
- Implement and test the canonical formatter/layer builder in core. Initialize
  it explicitly in both CLI packages with stderr writers, translating each
  front end's verbosity into the shared filter configuration.
- Replace ad hoc progress/status `eprintln!` calls in `glass-lint` with tracing
  events, and adapt harness progress events to the shared formatter without
  moving harness policy into core.
- Add black-box tests proving JSON stdout stays parseable at every verbosity,
  quiet mode suppresses INFO/DEBUG, verbose mode emits stage/timing markers to
  stderr, and no source/config contents leak into logs.
- Give the migrated harness CLI its own small verbosity option/config while
  using the same core formatter. It does not consume `CliConfig`, because
  profiling and adapter policy remain harness concerns.

### 5. Documentation and full validation

- Update `ARCHITECTURE.md`, `README.md`, `glass-lint-core/README.md`, both CLI
  package READMEs, `TESTING.md`, and `CONTRIBUTING.md` with the new package,
  config names/schema, examples, output contract, and stderr behavior.
- Add checked-in example `glass-lint.toml` and JSON snippets to documentation;
  do not place a live `glass-lint.toml` at the repository root unless it is
  intentionally the project's own default, because discovery would affect all
  repository-local examples and tests.
- Run targeted core/config/CLI tests while iterating, then run `make ci` as the
  completion gate.

## Completion criteria

The change is complete when:

1. `glass-lint-cli` has no dependency on `glass-lint-harness`, and the harness
   executable is built and invoked from `glass-lint-harness-cli`.
2. `CoreConfig` is public, provider-neutral, validated against a catalog, and
   applied by the CLI through the same API documented for Rust consumers.
3. With no config file, `glass-lint` uses documented defaults; cwd TOML, cwd
   JSON, explicit path, and inline JSON all produce the same validated config
   model with the documented precedence.
4. Default lint output is human-readable and every source excerpt is width
   bounded even for minified input; JSON output has the named, versioned
   envelope; and all solver/rule results are exclusively on stdout.
5. Logs, progress, timings, and operational errors are exclusively on stderr,
   including at `trace` verbosity, and redirected JSON remains valid.
6. Core exposes a provider-neutral pretty renderer, meaningful bounded tracing
   spans, and the formatter/subscriber factory used explicitly by both CLI
   packages, without automatically installing a subscriber or changing report
   schemas.
7. Output order and exit statuses are deterministic and covered by tests.
8. All affected commands and architectural documentation describe the new
   package and configuration contract, and `make ci` passes.
