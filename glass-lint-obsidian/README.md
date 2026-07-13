# glass-lint-obsidian

`glass-lint-obsidian` provides Glass Lint rules for detecting Obsidian API and
plugin capabilities. It owns Obsidian rule definitions, profile membership,
categories, and disclosure mappings; the generic semantic engine lives in
[`glass-lint-core`](../glass-lint-core/).

## Profiles

```rust
let precise = glass_lint_obsidian::recommended_linter();
let complete = glass_lint_obsidian::heuristic_linter();

let report = precise.lint(source, "main.js");
```

- `recommended_linter()` enables only high-confidence rules supported by
  provenance-aware or otherwise constrained matching.
- `heuristic_linter()` enables the complete catalog, including broad discovery
  rules.

Call `rule_catalog()` for serializable metadata. Rule IDs use the `obsidian:`
namespace, for example `obsidian:vault.read` and
`obsidian:network.request`.

## Disclosures

Use `disclosures_for_report(&report)` to derive the deterministic Obsidian
disclosure identifiers implied by a report. Disclosure policy remains in this
provider rather than leaking into core.

## Accuracy policy

The recommended profile is precision-first. Rules should prove module
provenance, global identity, rooted chains, constrained arguments, or connected
flow wherever possible. Broad member names, class names, suffix reads, and raw
literal fragments remain in the heuristic profile.

The promotion criteria and accepted matching mechanisms are documented in
[ACCURACY.md](ACCURACY.md).

## Adding a rule

Place each rule under `src/rules/<area>/<rule>/` with `positive.js` and
`negative.js` beside `mod.rs`. Describe its intended semantics and precision
boundary in a Rust doc comment, then cover direct behavior and relevant alias,
shadowing, reassignment, provenance, and lookalike cases.

```sh
cargo run -p glass-lint-cli --bin glass-lint-harness -- \
  verify glass-lint-obsidian/src/rules/<area>/<rule>
make test-rules
```

See [TESTING.md](../TESTING.md) for the complete fixture format.
