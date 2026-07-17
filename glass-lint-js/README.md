# glass-lint-js

`glass-lint-js` provides `js:` rules for browser, DOM, network, Node.js,
Electron, cryptography, archive, storage, and dynamic-code capabilities.

```rust
let report = glass_lint_js::recommended_linter()
    .lint_snippet(source, "bundle.js")?;
```

- `recommended_linter()` selects high-confidence rules.
- `heuristic_linter()` selects the complete catalog.
- `rule_catalog()` returns metadata for every rule.
- `disclosures_for_report()` derives sorted JavaScript disclosure identifiers.

`default_environment()` models the combined browser, Node.js, and Electron
globals used by the catalog. Extend it for additional host bindings:

```rust
let mut environment = glass_lint_js::default_environment();
environment.add_global("customRuntimeApi")?;

let linter =
    glass_lint_js::recommended_linter_with_environment(environment);
```

Use `add_global_object_with_members` when another realm exposes only known
members. The no-argument constructors use the default environment.

See [ARCHITECTURE.md](ARCHITECTURE.md) for provider boundaries and
[TESTING.md](../TESTING.md) for fixture conventions.
