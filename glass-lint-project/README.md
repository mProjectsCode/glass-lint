# glass-lint-project

`glass-lint-project` is Glass Lint's filesystem adapter. It owns bounded
source discovery and reads, canonical project boundaries, runtime-source
admission, `tsconfig.json` membership, and pinned Oxc resolution.

The crate exposes `ProjectLoader`, `ProjectLoadOptions`, and
`ProjectSelection`. It drives `glass-lint-core::ProjectSession` and passes
only owned sources and typed `ResolutionResult` values into core. Core never
receives a resolver, AST, or filesystem handle.

`ProjectLoader::load_and_lint_with_metrics` additionally returns bounded phase
timings and operation counts for profiling; it uses the same construction path
as `load_and_lint`.

The default loader supports `.js`, `.cjs`, `.mjs`, `.ts`, `.cts`, and `.mts`;
excludes declarations, `.git`, `node_modules`, common output directories, and
symlinked traversal; and bounds admitted files, source sizes, and resolution
requests. ESM-like requests use `node`/`import` conditions and CommonJS
`require()` uses `node`/`require` conditions. Unresolved bare packages remain
opaque external results, while unresolved internal requests fail closed.
