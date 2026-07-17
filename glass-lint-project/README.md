# glass-lint-project

`glass-lint-project` performs bounded filesystem discovery, source loading,
`tsconfig.json` membership, and Oxc-based module resolution. It supplies owned
sources and typed resolutions to `glass-lint-core`.

## Load and analyze

```rust
use glass_lint_project::{ProjectLoadOptions, ProjectLoader, ProjectSelection};

let options = ProjectLoadOptions::builder().build()?.validated()?;
let loader = ProjectLoader::new(options);
let selection = ProjectSelection::entry("src/main.ts");
let outcome = loader.load_and_lint(&linter, &selection)?;

if let Some(boundary) = outcome.error {
    eprintln!("partial project: {boundary}");
}
let report = outcome.report;
```

Use `ProjectSelection::entry` for one entry and reachable internal imports,
`directory` for all admitted sources below a directory, or `tsconfig` for
TypeScript configuration membership. Configure limits through the checked
`ProjectLoadOptions::builder()` before calling `validated()`.

## Policy

The default loader accepts `.js`, `.cjs`, `.mjs`, `.ts`, `.cts`, and `.mts`.
It excludes declarations, `.git`, `node_modules`, common output directories,
and symlink traversal.

Limits cover admitted files, bytes per file, aggregate bytes, visited entries,
resolver requests, and elapsed load time. Non-timeout resource boundaries
return deterministic partial output in `ProjectLoadOutcome`; a timeout is an
operational error.

ESM-like requests use `node` and `import` conditions. CommonJS requests use
`node` and `require`. Missing internal requests become diagnostics; unresolved
bare packages remain opaque external results.

See [ARCHITECTURE.md](ARCHITECTURE.md) for phase ownership and boundary
invariants.
