# Contributing

Glass Lint is an actively developed Rust workspace. Changes must preserve
provider boundaries, precision-first matching, bounded analysis, and
deterministic output.

Read [ARCHITECTURE.md](ARCHITECTURE.md) and the owning crate's
`ARCHITECTURE.md` before changing analysis, public APIs, or crate boundaries.
Read [TESTING.md](TESTING.md) before changing matcher or rule behavior.

## Prerequisites

- A recent Rust toolchain with Cargo, rustfmt, and Clippy
- GNU Make for repository shortcuts
- Bun only for the external ESLint adapter
- Samply only for profiling traces

## Workflow

1. Inspect `git status` and preserve unrelated changes.
2. Put the change in the owning crate.
3. Add the narrowest test that proves the behavior. Matching changes need
   focused positives and adversarial negatives.
4. Run a targeted test while iterating.
5. Update affected callers, fixtures, schemas, and documentation in the same
   change.
6. Run `make ci`.

Breaking changes are allowed during active development. Make one clean
migration and remove obsolete paths instead of adding compatibility layers by
default.

## Commands

| Command | Purpose |
|---|---|
| `make build` | Build the workspace |
| `make check` | Type-check the workspace |
| `make fmt` | Format Rust sources |
| `make fmt-check` | Check Rust formatting |
| `make clippy` | Run Clippy for all targets with warnings denied |
| `make test` | Run workspace unit and integration tests |
| `make test-e2e` | Verify `tests/e2e` |
| `make test-projects` | Verify `tests/projects` |
| `make test-rules` | Verify provider rule fixtures |
| `make ci` | Run the complete validation gate |
| `make profile` | Build and record a Samply profile |
| `make compare` | Regenerate `reports/COMPARISON.md` |

Override Make variables when needed:

```sh
make test-e2e HARNESS_SUITE=path/to/cases
make profile PROFILE_PATH=path/to/bundles PROFILE_PROVIDER=js
```

## Rule changes

Prefer the declarative matcher API in `glass-lint-core`. Add a reusable
provider-neutral matcher primitive when multiple rules need the same semantic
operation; use provider callbacks only when the matcher API cannot express the
rule accurately.

Rule factories use local IDs. `RuleCatalog` adds the namespace, producing IDs
such as `js:network.request`. Keep each provider contract beside its rule:

```text
rules/<area>/<rule>/
  mod.rs
  positive.js
  negative.js
```

Document the intended match, precision boundary, and known limitations in the
rule's Rust doc comment. [TESTING.md](TESTING.md) defines the required
adversarial coverage and fixture directives.

## Profiling and external comparison

Use `glass-lint-harness profile` for deterministic corpus selection, manifests,
worker comparisons, and phase metrics. Keep the build profile, verified
manifest, provider, rule profile, worker count, warm-up count, and repetition
count fixed across measurements. Prefer operation-count regression tests over
wall-clock assertions.

The [harness CLI README](glass-lint-harness-cli/) documents profiling modes.
The [ESLint adapter README](adapters/eslint-obsidianmd/) documents external
comparison setup.

## Documentation ownership

- Root `README.md`: user entry point
- Root `ARCHITECTURE.md`: crate graph and workspace boundaries only
- Crate `README.md`: public purpose and usage
- Crate `ARCHITECTURE.md`: internal design and invariants
- `TESTING.md`: test placement and fixture authoring
- `AGENTS.md`: concise coding-agent instructions

Keep one source of truth and link to it instead of copying it.
