# glass-lint-obsidian

`glass-lint-obsidian` provides the `obsidian:` rule catalog and renderer host
assumptions.

```rust
let linter = glass_lint_core::Linter::new(glass_lint_obsidian::obsidian_config())?;
let report = linter.lint_source(glass_lint_core::project::SourceFile::new("main.js", source)?)?;
```

- `obsidian_catalog()` returns the isolated Obsidian catalog.
- `obsidian_environment()` returns the complete Obsidian renderer environment.
- `rule_metadata()` returns metadata for every Obsidian rule.

This crate does not include the `js:` catalog. The command-line front end
combines both catalogs when its provider is `obsidian`.

`obsidian_environment()` models configured globals in the Obsidian Electron
renderer, including `app`, `activeDocument`, `Notice`, `moment`, `request`, and
`requestUrl`. It treats `activeWindow` as a global-object alias because static
analysis cannot determine whether it represents the main or a pop-out window.

Extend the environment for additional host bindings:

```rust
let mut environment = glass_lint_obsidian::obsidian_environment();
environment.add_global("customPluginHost")?;

let linter = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
    vec![glass_lint_obsidian::obsidian_catalog()], environment,
))?;
```

See [ACCURACY.md](ACCURACY.md) for profile policy,
[ARCHITECTURE.md](ARCHITECTURE.md) for crate boundaries, and
[TESTING.md](../TESTING.md) for fixtures.
