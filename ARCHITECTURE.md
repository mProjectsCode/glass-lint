# Workspace architecture

Glass Lint is a Rust workspace with one provider-neutral engine, separate
filesystem and policy layers, and thin command-line front ends.

## Crate graph

Arrows point from a crate to its workspace dependency:

```text
glass-lint-cli ─────────┬──> glass-lint-project ──> glass-lint-core
                       ├──> glass-lint-js ────────> glass-lint-core
                       ├──> glass-lint-obsidian ──> glass-lint-core
                       └──────────────────────────> glass-lint-core

glass-lint-harness-cli ─┬──> glass-lint-harness ──┬──> glass-lint-project
                       │                          ├──> glass-lint-js
                       │                          ├──> glass-lint-obsidian
                       │                          └──> glass-lint-core
                       └──────────────────────────> glass-lint-core
```

Provider crates do not depend on each other. `glass-lint-project` knows
nothing about providers. Production crates do not depend on either harness
crate.

## Workspace boundaries

| Crate | Owns | Does not own |
|---|---|---|
| [`glass-lint-core`](glass-lint-core/ARCHITECTURE.md) | Parsing, provider-neutral semantics, matchers, project linking, limits, and reports | Filesystem access, module resolution, host policy, profiles, or CLI behavior |
| [`glass-lint-project`](glass-lint-project/ARCHITECTURE.md) | Discovery, source loading, project boundaries, `tsconfig` membership, and module resolution | Parsing, semantic matching, or provider policy |
| [`glass-lint-js`](glass-lint-js/ARCHITECTURE.md) | `js:` rules, JavaScript runtime assumptions, profiles, and disclosures | Generic analysis or filesystem behavior |
| [`glass-lint-obsidian`](glass-lint-obsidian/ARCHITECTURE.md) | `obsidian:` rules, Obsidian host assumptions, profiles, and disclosures | Generic analysis or the `js:` catalog |
| [`glass-lint-cli`](glass-lint-cli/ARCHITECTURE.md) | User configuration, command dispatch, presentation, and exit status | Reusable analysis, loading, or rule logic |
| [`glass-lint-harness`](glass-lint-harness/ARCHITECTURE.md) | Cases, adapters, verification, comparison reports, and profiling | Production CLI policy |
| [`glass-lint-harness-cli`](glass-lint-harness-cli/ARCHITECTURE.md) | Harness arguments, output, and executable wiring | Case parsing, comparison semantics, or profiling logic |

Shared semantics move toward `glass-lint-core`; host policy stays in provider
crates; filesystem policy stays in `glass-lint-project`; executable crates
remain thin. Do not duplicate parsers, semantic models, matcher paths, project
loaders, report types, or rule catalogs across these boundaries.
