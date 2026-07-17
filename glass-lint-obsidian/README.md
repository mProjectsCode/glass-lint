# glass-lint-obsidian

`glass-lint-obsidian` provides the `obsidian:` rule catalog, renderer host
assumptions, and disclosure mapping.

```rust
let linter = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
    vec![glass_lint_obsidian::catalog()], glass_lint_obsidian::environment(),
))?;
let report = linter.lint_snippet(source, "main.js")?;
```

- `catalog()` returns the isolated Obsidian catalog.
- `environment()` returns the complete Obsidian renderer environment.
- `rule_catalog()` returns metadata for every Obsidian rule.
- `disclosures_for_report()` derives sorted Obsidian disclosure identifiers.

This crate does not include the `js:` catalog. The command-line front end
combines both catalogs when its provider is `obsidian`.

`environment()` models configured globals in the Obsidian Electron
renderer, including `app`, `activeDocument`, `Notice`, `moment`, `request`, and
`requestUrl`. It treats `activeWindow` as a global-object alias because static
analysis cannot determine whether it represents the main or a pop-out window.

Extend the environment for additional host bindings:

```rust
let mut environment = glass_lint_obsidian::environment();
environment.add_global("customPluginHost")?;

let linter = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
    vec![glass_lint_obsidian::catalog()], environment,
))?;
```

See [ACCURACY.md](ACCURACY.md) for profile policy,
[ARCHITECTURE.md](ARCHITECTURE.md) for crate boundaries, and
[TESTING.md](../TESTING.md) for fixtures.
