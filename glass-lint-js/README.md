# glass-lint-js

`glass-lint-js` provides separate `js:`, `browser:`, `node:`, and `electron:`
catalogs for browser, DOM, network, Node.js,
Electron, cryptography, archive, storage, and dynamic-code capabilities.

```rust
let linter = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
    vec![glass_lint_js::js_catalog()], glass_lint_js::js_environment(),
))?;
let report = linter.lint_source(glass_lint_core::project::SourceFile::new("bundle.js", source)?)?;
```

- `js_catalog()`, `browser_catalog()`, `node_catalog()`, and
  `electron_catalog()` return isolated catalogs.
- The matching `*_environment()` functions return complete host environments.
- `rule_metadata()` returns metadata for every JavaScript-provider rule.

Extend a complete host environment for additional bindings:

```rust
let mut environment = glass_lint_js::electron_environment();
environment.add_global("customRuntimeApi")?;

let linter = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
    vec![glass_lint_js::electron_catalog()], environment,
))?;
```

Use `add_global_object_with_members` when another realm exposes only known
members. The no-argument environment functions return complete built-in host
environments.

See [ARCHITECTURE.md](ARCHITECTURE.md) for provider boundaries and
[TESTING.md](../TESTING.md) for fixture conventions.
