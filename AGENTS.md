# Glass Lint agent guide

## Before editing

- Inspect `git status` and preserve unrelated changes.
- Read [`ARCHITECTURE.md`](ARCHITECTURE.md) before changing analysis, matchers,
  crate boundaries, or public APIs.
- Read [`TESTING.md`](TESTING.md) before changing matching behavior or rules.
  Use [`CONTRIBUTING.md`](CONTRIBUTING.md) for commands and tooling.

## Put changes in the owning crate

- `glass-lint-core`: provider-neutral parsing, semantics, flow, matchers,
  catalogs, and reports
- `glass-lint-project`: discovery, source loading, project boundaries,
  `tsconfig`, and module resolution
- `glass-lint-js`: JavaScript, browser, Node.js, and Electron policy
- `glass-lint-obsidian`: Obsidian rules, profiles, and disclosures
- `glass-lint-harness`: cases, adapters, verification, reports, and profiling
- CLI crates: arguments, output, exit behavior, and executable wiring only

Core must not contain provider names, APIs, categories, manifests,
disclosures, profiles, or rule policy. Rule IDs are `provider:name`, such as
`js:network.request`.

## Implementation rules

- Prefer declarative matchers. Add reusable semantics to core; use a provider
  callback only when the matcher API cannot express the rule accurately.
- Preserve strict identity. Shadowing, reassignment, local lookalikes, dynamic
  values, ambiguity, unsupported semantics, and exhausted budgets fail closed.
- Parse and build matcher-independent facts once per file. A rule must not add
  its own traversal or semantic model.
- Keep analysis bounded and output deterministic, including locations and
  evidence order.
- Put behavior on the type that owns the state. Use free functions for genuine
  coordination across independent types.
- Prefer semantic newtypes and domain collections when they enforce invariants,
  clarify meaning, or encapsulate repeated map/set/index operations.
- Keep modules cohesive, functions single-level, and public APIs small and
  validated. Do not expose internal storage for caller convenience.
- Centralize domain logic and naming. Do not add duplicate parsers, model
  types, matcher paths, reports, or compatibility wrappers.
- Model expected errors explicitly. Do not panic on unsupported input or
  resource exhaustion.
- Delete obsolete paths after migrations; update all callers in the same
  change.

## Tests and completion

- Add focused positives and adversarial negatives for matching changes. Cover
  relevant shadowing, lookalikes, aliases, reassignment, imports, dynamic
  values, flow lifecycle, and minified shapes.
- Put reusable matcher tests in `glass-lint-core/tests`, provider contracts
  beside `positive.js` and `negative.js`, and cross-rule workflows in
  `tests/e2e`.
- Run a narrow test while iterating, then run the full gate:

  ```sh
  make ci
  ```

- Breaking changes are allowed, but must update every caller, fixture, adapter,
  schema, test, and document. Make one clean path, not a compatibility layer.
