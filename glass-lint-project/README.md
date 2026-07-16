# glass-lint-project

`glass-lint-project` turns a filesystem selection into a bounded, resolver-aware
analysis. It owns source discovery and reads, canonical project boundaries,
`tsconfig.json` membership, and Oxc-based module resolution.

The crate passes owned sources and typed resolution results into the analysis
engine. Resolver handles, filesystem handles, and resolver-specific types do
not cross its public boundary.

## Load a project

Choose one entry and its reachable internal imports, every admitted source in a
directory, or the files selected by a TypeScript configuration:

```rust
use glass_lint_project::{ProjectLoadOptions, ProjectLoader, ProjectSelection};

let loader = ProjectLoader::new(ProjectLoadOptions::default())?;
let selection = ProjectSelection::entry("src/main.ts");
let report = loader.load_and_lint(&linter, &selection)?;
```

The other constructors are `ProjectSelection::directory` and
`ProjectSelection::tsconfig`. The selection determines the default project
root; set `ProjectLoadOptions::root` when the boundary must be explicit.

`load_and_lint_with_outcome` preserves a deterministic partial report when a
resource boundary is reached. `load_and_lint_with_metrics` returns phase
timings and operation counts for profiling.

## Discovery and resolution policy

By default, the loader admits `.js`, `.cjs`, `.mjs`, `.ts`, `.cts`, and `.mts`.
It excludes declarations, `.git`, `node_modules`, common output directories,
and symlinked traversal. Limits cover admitted files, bytes per source,
aggregate source bytes, visited filesystem entries, resolver requests, and
total elapsed loading time.

ESM-like requests use the `node` and `import` conditions. CommonJS `require()`
uses `node` and `require`. Unresolved bare packages remain opaque external
results; unresolved internal requests remain diagnostics and never become
guessed provenance.

`ProjectLoadOptions::validate` rejects invalid limits, source suffixes, and
extension aliases before I/O begins.
