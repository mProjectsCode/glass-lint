# JavaScript provider architecture

`glass-lint-js` is a policy crate over `glass-lint-core`. It has no filesystem
or command-line responsibilities.

```text
rule factories + JavaScript host environment
  -> namespaced RuleCatalogs (`js:`, `browser:`, `node:`, `electron:`)
  -> caller-selected core Linter configuration
  -> JavaScript disclosure mapping
```

Rules are grouped by runtime area under `src/rules`. Each factory returns a
provider-neutral core `Rule` with a local ID; the four exported catalogs add
their own namespace. Environment builders model complete host targets, with
browser and Node extending JavaScript and Electron extending both.

Callers choose a catalog and environment, then apply their own baseline and
overrides through core. Disclosure derivation is a provider policy applied
after core produces a report.

Shared semantics belong in core. This crate owns rule metadata, matcher
composition, host assumptions, fixtures, and disclosure mapping only. Named
selection policy belongs to callers. It must not traverse an AST, load files,
resolve modules, or define a competing report type.
