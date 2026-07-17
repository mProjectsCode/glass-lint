# Obsidian provider architecture

`glass-lint-obsidian` is a policy crate over `glass-lint-core`. It owns only the
`obsidian:` namespace; combining it with the `js:` catalog is a front-end
choice.

```text
Obsidian rule factories + renderer environment
  -> namespaced RuleCatalog (`obsidian:`)
  -> confidence selection
  -> core Linter
  -> Obsidian disclosure mapping
```

`src/rules` contains rule factories and colocated fixtures. `src/catalog`
collects those rules and maps findings to disclosure identifiers.
`default_environment` models configured Obsidian renderer globals and
global-object aliases, including `activeWindow`.

`recommended_linter` contains high-confidence rules whose fixtures establish
the relevant identity, provenance, value, shadowing, and reassignment
boundaries. `heuristic_linter` adds broader syntactic and literal discovery.
No numeric precision claim is made without a representative manually labeled
corpus.

Reusable semantics belong in core. This crate must not add its own traversal,
semantic model, project loader, or report type. Provider callbacks are reserved
for behavior that cannot be represented accurately with declarative core
matchers.
