# JavaScript provider architecture

`glass-lint-js` is a policy crate over `glass-lint-core`. It has no filesystem
or command-line responsibilities.

```text
rule factories + JavaScript host environment
  -> namespaced RuleCatalog (`js:`)
  -> confidence selection
  -> core Linter
  -> JavaScript disclosure mapping
```

Rules are grouped by runtime area under `src/rules`. Each factory returns a
provider-neutral core `Rule` with a local ID; catalog construction validates
the rules and adds the `js:` namespace. `default_environment` supplies the
browser, Node.js, and Electron globals assumed by the combined catalog.

`recommended_linter` enables high-confidence rules.
`heuristic_linter` enables the complete catalog. Disclosure derivation is a
provider policy applied after core produces a report.

Shared semantics belong in core. This crate owns rule metadata, matcher
composition, host assumptions, profile membership, fixtures, and disclosure
mapping only. It must not traverse an AST, load files, resolve modules, or
define a competing report type.
