# Glass Lint code structure

This is a source-oriented catalog of the production Rust workspace. It was
checked with `cargo modules structure --no-fns --no-traits` for every Cargo
target and then refined by reading the owning source modules and architecture
documents.

The catalog names every production module, struct, and enum reported by
`cargo-modules`; each entry has one plain-language sentence describing its job
in the system. Type aliases are shown only when they clarify a module's
boundary, and functions and traits are intentionally omitted.

Test-only modules and test helper types are not included because they verify
the runtime system rather than implement it. The CLI binary roots are listed
separately from their library targets because `cargo-modules` treats them as
distinct targets.

## Workspace flow

`glass-lint-datastructures` supplies bounded storage to `glass-lint-core`,
which performs parsing, semantic analysis, matching, project linking, and
report construction. `glass-lint-project` adds filesystem discovery and
resolution; `glass-lint-js` and `glass-lint-obsidian` add provider policy;
`glass-lint-output` renders reports; and the harness and CLI crates provide
verification, profiling, and executable wiring.

- [Core engine catalog](CODEBASE_STRUCTURE_CORE.md)
- [Datastructures, project, provider, output, and harness catalog](CODEBASE_STRUCTURE_LIBRARIES.md)
- [CLI and harness-CLI catalog](CODEBASE_STRUCTURE_CLIS.md)
