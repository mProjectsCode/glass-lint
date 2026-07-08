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

## Harness Cases

Harness cases are ordinary `.js` files. Put related rule cases in topic folders, for example `tests/cases/network/*.js` for network behavior and `tests/cases/system/*.js` for dynamic-code or timer behavior. The case ID is the path below the suite root without `.js`, unless the file sets `// @case id ...`.

The default runnable suite is `tests/cases`. Ports of old adversarial cases that describe behavior the current linter does not yet satisfy live in `tests/cases-regressions`; run that suite directly when working on those precision gaps.

Configuration comments must be at the very top of the file, before executable code:

```js
// @case description Each fetch call produces a located finding
// @case tags network,browser
// @tool glass-lint rules=obsidian:network.browser
// @tool eslint-obsidianmd rules=obsidianmd/no-global-this
```

Only configured tools run a case. When a report includes a registered tool that is not mentioned by a case, that tool is marked `skip` for that case instead of failing coverage.

Expected diagnostics use assertion comments next to the relevant source line:

```js
// @expect-error glass-lint rule=obsidian:network.browser message_id=detected
fetch('/before');

fetch('/inline'); // @expect-error glass-lint rule=obsidian:network.browser message_id=detected

fetch('/after');
// @expect-error-after glass-lint rule=obsidian:network.browser message_id=detected
```

Supported assertion fields are `rule`, `message_id`, `severity`, `count`, `line`, `column`, and `message`. Use `count=any`, `line=any`, or `column=any` only when the old behavior being preserved is aggregate capability presence rather than exact evidence shape or location. Prefer one assertion comment per expected diagnostic and keep fields as specific as needed for precision. A case with configured rules and no assertions verifies that the selected tool produces no diagnostics.
