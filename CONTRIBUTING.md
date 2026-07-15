# Contributing

Glass Lint is a Rust workspace under active development. Contributions should
preserve its provider boundaries, precision-first matching, bounded analysis,
and deterministic output.

Read [ARCHITECTURE.md](ARCHITECTURE.md) before changing analysis or matcher
behavior. Read [TESTING.md](TESTING.md) before adding a rule or changing an
existing match boundary.

## Prerequisites

- A recent Rust toolchain with `cargo`, `rustfmt`, and Clippy
- GNU Make for the documented shortcuts
- Bun only when working on the ESLint comparison adapter
- Samply only when collecting profiling traces

Build the complete workspace with:

```sh
make build
```

## Development workflow

1. Identify the owning crate before editing. Generic analysis belongs in
   `glass-lint-core`; filesystem discovery and Oxc resolution belong in
   `glass-lint-project`; JavaScript platform and Obsidian policy belong in
   their provider crates.
2. Add or update focused tests with the implementation. Matching changes need
   positive cases and adversarial negatives.
3. Run the narrowest relevant test while iterating.
4. Run the full validation gate before considering the change complete.
5. Update public documentation, fixtures, adapters, and callers when making a
   breaking change.

The full local gate is:

```sh
make ci
```

This checks formatting, compilation, warnings-denied Clippy, workspace tests,
end-to-end harness cases, and both providers' rule fixtures.

## Make targets

| Target | Purpose |
|---|---|
| `make build` | Build every workspace package |
| `make check` | Type-check the workspace |
| `make fmt` | Format Rust sources |
| `make fmt-check` | Verify Rust formatting without modifying files |
| `make clippy` | Run Clippy for all targets with warnings denied |
| `make test` | Run all Rust unit and integration tests |
| `make test-e2e` | Verify `tests/e2e` through the harness |
| `make test-projects` | Verify virtual and filesystem project fixtures |
| `make test-rules` | Verify colocated JavaScript and Obsidian rule fixtures |
| `make profile` | Build the profiling harness and record with Samply |
| `make compare` | Compare Glass Lint with the external ESLint adapter |
| `make ci` | Run the complete required validation gate |
| `make clean` | Remove Cargo build artifacts |

Override Make variables on the command line when needed. For example:

```sh
make test-e2e HARNESS_SUITE=path/to/cases
make profile PROFILE_PATH=path/to/bundles PROFILE_PROVIDER=js
```

## Adding or changing rules

Use the declarative matcher API from `glass-lint-core` whenever possible. A
typical provider rule supplies a local rule ID, label, category, severity,
confidence, and one or more matchers. The catalog adds the provider prefix.

Choose the strongest matcher supported by the intended semantics:

- global or module-provenance matchers for identified calls and constructors;
- rooted member matchers for proven object chains;
- argument constraints for static values or rooted expressions;
- connected flow matchers when source, transformation, and sink must coexist;
- syntactic heuristics only for opt-in discovery behavior.

Do not use raw strings or property names when the rule claims a proven API
identity. Add shared matcher capability to core if several providers need the
same semantic operation.

Place provider fixture files beside the rule implementation:

```text
rules/<area>/<rule>/
  mod.rs
  positive.js
  negative.js
```

Document the rule's intended matches, precision boundary, and known
limitations in a Rust doc comment.

## Profiling

Folder profiling reads selected JavaScript and TypeScript runtime files into memory before timing `Linter::lint`,
so measured lint wall time excludes discovery, file reads, and decoding. File
discovery is recursive and deterministic and does not follow symlinks.

Use one representative subfolder of a production corpus rather than an entire
release archive:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- profile \
  --path /path/to/plugin/bundles \
  --provider obsidian \
  --profile recommended \
  --sample 20 \
  --seed 20260712 \
  --quiet
```

Repeated `--path` options combine roots. `--include` and `--exclude` use
`glob::Pattern` syntax with slash-separated paths; a pattern without a slash
also matches basenames. Use `--warm-up`, `--repeat`, and `--workers` to control
execution. The default is one measured pass on one worker.

For a Samply trace:

```sh
make profile \
  PROFILE_PATH=/path/to/plugin/bundles \
  PROFILE_ARGS="--quiet --sample 20 --seed 20260712"
```

Performance changes should compare the same build profile, sample, seed,
worker count, and repeat policy. Prefer deterministic operation-count tests to
wall-clock assertions in the normal suite.

For project profiling, add `--project`. The summary reports discovery, reads,
parse/local analysis, resolution, linking/matching, and total time, together
with deterministic counts for files, requests, edges, and evidence. Use a
representative mixed-language project and keep resolver/network work out of
the timed corpus setup when comparing runs.

## External comparison adapter

The adapter under `adapters/eslint-obsidianmd` requires Bun dependencies:

```sh
cd adapters/eslint-obsidianmd
bun install
cd ../..
make compare
```

The comparison command writes [`reports/COMPARISON.md`](reports/COMPARISON.md).
See the [adapter README](adapters/eslint-obsidianmd/README.md) for protocol and
isolation details.

## Documentation

Keep docs close to their audience:

- `README.md` is the user-facing entry point.
- Each crate README explains that crate's purpose and public surface.
- `ARCHITECTURE.md` records durable boundaries and data flow.
- `TESTING.md` defines test layers and fixture authoring.
- `AGENTS.md` contains concise repository instructions for coding agents.

When commands, rule IDs, schemas, or directory layouts change, update every
affected document in the same change.
