# glass-lint-obsidian

`glass-lint-obsidian` provides rules for Obsidian APIs and plugin capabilities.
It owns the `obsidian:` rule catalog, confidence profiles, renderer globals,
and disclosure mapping.

## Choose a profile

```rust
let linter = glass_lint_obsidian::recommended_linter();
let report = linter.lint(source, "main.js");
```

`recommended_linter()` selects rules backed by high-confidence matching.
`heuristic_linter()` selects the complete catalog, including broad discovery
rules. `rule_catalog()` returns serializable metadata for every rule.

Rule IDs use the `obsidian:` namespace, for example `obsidian:vault.read` and
`obsidian:network.request`.

## Obsidian runtime globals

`default_environment()` describes globals available to an Obsidian plugin in
the Electron renderer, including `app`, `activeDocument`, `Notice`, `moment`,
`request`, and `requestUrl`.

`activeWindow` may refer to the main window or a pop-out window. Static
analysis cannot determine which realm it represents at a call site, so the
default environment treats it as a global-object alias with the same configured
identities.

Extend the environment when analyzing a host with additional injected names:

```rust
let mut environment = glass_lint_obsidian::default_environment();
environment.add_global("customPluginHost")?;

let linter =
    glass_lint_obsidian::recommended_linter_with_environment(environment);
```

The no-argument constructors use the crate's default environment.

## Derive disclosures

`disclosures_for_report(&report)` returns the sorted Obsidian disclosure
identifiers implied by `obsidian:` findings. Findings from other namespaces are
ignored.

The recommended profile is precision-first: rules should establish provenance,
global identity, constrained arguments, or connected flow. Broad member names,
class names, suffix reads, and literal fragments belong in the heuristic
profile. [ACCURACY.md](ACCURACY.md) documents the promotion criteria.

Rule fixtures live beside their implementations. See the repository
[testing guide](../TESTING.md) for authoring and validation conventions.
