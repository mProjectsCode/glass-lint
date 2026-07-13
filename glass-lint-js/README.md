# glass-lint-js

`glass-lint-js` is the Glass Lint provider for generic JavaScript runtime
capabilities. Its catalog covers browser, DOM, network, Node.js, Electron,
cryptography, archive, and dynamic-code behavior.

The crate contains rule policy only. Parsing, scope analysis, provenance,
value flow, matcher execution, and report construction live in
[`glass-lint-core`](../glass-lint-core/).

## Profiles

```rust
let precise = glass_lint_js::recommended_linter();
let complete = glass_lint_js::heuristic_linter();

let report = precise.lint(source, "bundle.js");
```

- `recommended_linter()` enables rules classified with high-confidence
  matching mechanisms.
- `heuristic_linter()` enables the entire JavaScript catalog, including broad
  literal and syntactic discovery rules.

Call `rule_catalog()` to obtain serializable metadata for every rule. Rule IDs
are namespaced with `js:`, such as `js:network.request`.

## Disclosures

`disclosures_for_report(&report)` maps detected JavaScript capabilities to a
deterministic set of disclosure identifiers. The provider owns this mapping;
core reports remain policy-neutral.

## Adding a rule

Place the implementation under `src/rules/<area>/<rule>/` with colocated
`positive.js` and `negative.js` harness fixtures. Prefer strict global,
module-provenance, rooted, or connected-flow matchers. Name-only and broad
literal matching belongs in the heuristic profile.

Run the focused fixture and provider suite:

```sh
cargo run -p glass-lint-cli --bin glass-lint-harness -- \
  verify glass-lint-js/src/rules/<area>/<rule>
make test-rules
```

See [TESTING.md](../TESTING.md) for fixture syntax and required adversarial
coverage.
