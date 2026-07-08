# Glass Lint

Glass Lint is a general JavaScript lint engine, an Obsidian rule pack, and a snippet-first conformance harness.

```sh
cargo run -p glass-lint-cli --bin glass-lint -- rules
cargo run -p glass-lint-cli --bin glass-lint -- check main.js
cargo run -p glass-lint-cli --bin glass-lint-harness -- verify tests/cases
```

`glass-lint-core` owns parsing, provenance and alias-flow analysis, declarative rule matching, configuration, and reports. It contains no Obsidian names or policy. `glass-lint-obsidian` contains only its private rule catalog and exposes ready-to-use precision-first and heuristic linters plus catalog metadata. Rule IDs use `provider:name`, such as `obsidian:network.browser`.

```rust
let report = glass_lint_obsidian::recommended_linter()
    .lint(source, "main.js");

let configured = glass_lint_obsidian::heuristic_linter();
let selected = [glass_lint_core::RuleId::parse("obsidian:network.browser")?];
let custom = glass_lint_core::Linter::with_rules(
    configured.catalog().clone(),
    selected,
)?;
```

The parser accepts JavaScript (including JSX). TypeScript, fixes, and suggestions are intentionally out of scope.
