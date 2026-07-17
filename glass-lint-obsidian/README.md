# glass-lint-obsidian

`glass-lint-obsidian` provides the `obsidian:` rule catalog, renderer host
assumptions, confidence profiles, and disclosure mapping.

```rust
let report = glass_lint_obsidian::recommended_linter()
    .lint_snippet(source, "main.js")?;
```

- `recommended_linter()` selects high-confidence Obsidian rules.
- `heuristic_linter()` selects the complete Obsidian catalog.
- `rule_catalog()` returns metadata for every Obsidian rule.
- `disclosures_for_report()` derives sorted Obsidian disclosure identifiers.

This crate does not include the `js:` catalog. The command-line front end
combines both catalogs when its provider is `obsidian`.

`default_environment()` models configured globals in the Obsidian Electron
renderer, including `app`, `activeDocument`, `Notice`, `moment`, `request`, and
`requestUrl`. It treats `activeWindow` as a global-object alias because static
analysis cannot determine whether it represents the main or a pop-out window.

Extend the environment for additional host bindings:

```rust
let mut environment = glass_lint_obsidian::default_environment();
environment.add_global("customPluginHost")?;

let linter =
    glass_lint_obsidian::recommended_linter_with_environment(environment);
```

See [ACCURACY.md](ACCURACY.md) for profile policy,
[ARCHITECTURE.md](ARCHITECTURE.md) for crate boundaries, and
[TESTING.md](../TESTING.md) for fixtures.
