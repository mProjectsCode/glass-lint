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

## Runtime environment

`default_environment()` describes the globals available in Obsidian's Electron
renderer, including `app`, `activeDocument`, `Notice`, `moment`, `request`, and
`requestUrl`.

Pop-out windows do not expose every global available in the main window. The
analyzer nevertheless gives `activeWindow` the same environment because it
cannot know at analysis time whether that value refers to the main window or a
pop-out window.

Extend the environment when targeting another host:

```rust
let mut environment = glass_lint_obsidian::default_environment();
environment.add_global("customPluginHost")?;
let linter =
    glass_lint_obsidian::recommended_linter_with_environment(environment);
```

The no-argument profile constructors use this provider default.

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

For rule authoring, fixture placement, and validation, see the repository
[contributing guide](../CONTRIBUTING.md) and [testing guide](../TESTING.md).
