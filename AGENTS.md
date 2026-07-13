# Glass Lint agent guide

## Start here

- Read [`ARCHITECTURE.md`](ARCHITECTURE.md) before changing analysis,
  matchers, package boundaries, or public APIs.
- Read [`TESTING.md`](TESTING.md) before changing matching behavior or adding a
  rule. Use [`CONTRIBUTING.md`](CONTRIBUTING.md) for the complete tooling and
  validation commands.
- Inspect the worktree before editing. Preserve unrelated user changes and do
  not revert or reformat them.

## Ownership boundaries

- `glass-lint-core` is the provider-neutral JavaScript engine. It owns parsing,
  semantic facts, scope and shadowing analysis, provenance, alias and value
  flow, matcher compilation and execution, rule catalogs, and generic reports.
- Core must not contain Obsidian names, APIs, categories, manifests,
  disclosures, profiles, or rule policy. Generic JavaScript, browser, Node.js,
  and Electron policy belongs in `glass-lint-js`.
- `glass-lint-obsidian` owns Obsidian rules, profiles, disclosure mappings, and
  the few provider-specific semantics that cannot be expressed generically.
- `glass-lint-harness` owns case parsing, adapters, verification, comparison
  reports, and profiling. `glass-lint-cli` owns thin executable front ends.
- Rule IDs use `provider:name`, for example `js:network.request` and
  `obsidian:network.request`.

## Implementation rules

- Prefer the declarative core matcher API for provider rules. Add a reusable
  matcher primitive to core when multiple rules need the same semantics; use a
  provider-specific Rust callback only when a declarative rule cannot be
  accurate.
- Preserve precision-first behavior. Strict matches require lexical identity,
  supported provenance, or connected flow at the use position. Raw names,
  suffixes, and broad literal fragments require an explicit heuristic mode.
- Unknown, dynamic, ambiguous, unsupported, or budget-exhausted semantics fail
  closed. Do not leak facts across bindings, assignments, scopes, or control
  paths.
- Parse once and build matcher-independent semantic indexes once per file.
  Selecting or adding a rule must not add an AST traversal or change fact
  construction.
- Keep analysis and reports bounded and deterministic. Preserve finding order,
  evidence order, and exact source locations.
- Keep modules focused and public APIs small, validated, and difficult to
  misuse. Prefer types with clear invariants over loosely related helpers.
- Do not create duplicate parsers, semantic models, matcher paths, report
  types, or compatibility wrappers unless explicitly requested.

## Tests and completion

- Add focused positives and adversarial negatives for matching changes. Cover
  relevant shadowing, local lookalikes, aliases, reassignment, imports,
  dynamic values, flow lifecycle, and minified bundle shapes.
- Put reusable matcher behavior in `glass-lint-core/tests`, provider rule
  contracts beside their `positive.js` and `negative.js` fixtures, and
  cross-rule workflows in `tests/e2e`.
- Run a narrow test while iterating, then run the complete gate before handing
  off a finished change:

  ```sh
  make ci
  ```

- Breaking Rust APIs, JSON schemas, rule IDs, and layouts are allowed while
  the project is in active development. Make a clean break: update all callers,
  fixtures, adapters, tests, and documentation in the same change.
