# Obsidian provider architecture

`glass-lint-obsidian` is a policy crate over `glass-lint-js` and
`glass-lint-core`. It owns only the `obsidian:` namespace; callers compose it
with the JavaScript provider catalogs under the complete Obsidian renderer
environment.

```text
Obsidian rule factories + renderer environment
  -> namespaced RuleCatalog (`obsidian:`)
  -> caller-selected core Linter configuration
  -> Obsidian disclosure mapping
```

`src/rules` contains rule factories and colocated fixtures. `src/catalog`
collects those rules and maps findings to disclosure identifiers.
`environment` models configured Obsidian renderer globals and
global-object aliases, including `activeWindow`.

Callers choose the catalog and apply rule-selection policy through core. No
numeric precision claim is made without a representative manually labeled
corpus.

Reusable semantics belong in core. This crate must not add its own traversal,
semantic model, project loader, or report type. Provider callbacks are reserved
for behavior that cannot be represented accurately with declarative core
matchers.
