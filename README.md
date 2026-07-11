# Glass Lint

Glass Lint is a general JavaScript lint engine, an Obsidian rule pack, and a snippet-first conformance harness.

```sh
cargo run -p glass-lint-cli --bin glass-lint -- rules
cargo run -p glass-lint-cli --bin glass-lint -- check main.js
cargo run -p glass-lint-cli --bin glass-lint -- --provider js check main.js
cargo run -p glass-lint-cli --bin glass-lint-harness -- verify tests/e2e
```

`glass-lint-core` owns parsing, provenance and alias-flow analysis, declarative rule matching, configuration, and reports. It contains no product policy. `glass-lint-obsidian` owns Obsidian rules, while `glass-lint-js` owns generic JavaScript, browser, Node.js, and Electron rules. Rule IDs use `provider:name`, such as `obsidian:network.request` and `js:network.request`.

```rust
let report = glass_lint_obsidian::recommended_linter()
    .lint(source, "main.js");

let configured = glass_lint_obsidian::heuristic_linter();
let selected = [glass_lint_core::RuleId::parse("obsidian:network.request")?];
let custom = glass_lint_core::Linter::with_rules(
    configured.catalog().clone(),
    selected,
)?;
```

The parser accepts JavaScript (including JSX). TypeScript, fixes, and suggestions are intentionally out of scope.

## Harness Cases

Harness cases are ordinary `.js` files. Rule-level conformance fixtures live alongside their Rust definitions as `positive.js` and `negative.js`; run `make provider-fixtures` to verify them. The remaining `tests/` tree is reserved for end-to-end scenarios, and `make harness` runs `tests/e2e` by default.

The case ID is the path below the suite root without `.js`, unless the file sets `// @case id ...`.

Configuration comments must be at the very top of the file, before executable code:

```js
// @case description Each fetch call produces a located finding
// @case tags network,browser
// @tool glass-lint rules=js:network.request
// @tool glass-lint config=heuristic
// @tool eslint-obsidianmd rules=obsidianmd/no-global-this
```

Only configured tools run a case. When a report includes a registered tool that is not mentioned by a case, that tool is marked `skip` for that case instead of failing coverage.
The built-in `glass-lint` adapter accepts `config=heuristic` to run the complete
JavaScript and Obsidian rule catalogs. Use explicit `rules=` for focused rule
fixtures.

Expected diagnostics use assertion comments next to the relevant source line:

```js
// @expect-error glass-lint rule=js:network.request message_id=detected
fetch('/before');

fetch('/inline'); // @expect-error glass-lint rule=js:network.request message_id=detected

fetch('/after');
// @expect-error-after glass-lint rule=js:network.request message_id=detected
```

Use `@expect-no-error` (or `@expect-no-error-after`) to assert that a selected rule
does not report a particular lookalike while allowing other expected diagnostics in
the same snippet:

```js
fetch('/remote'); // @expect-error glass-lint rule=js:network.request
function local(fetch) { fetch('/local'); } // @expect-no-error glass-lint rule=js:network.request
```

Supported assertion fields are `rule`, `message_id`, `severity`, `count`, `line`, `column`, and `message`. Use `count=any`, `line=any`, or `column=any` only when the behavior under test is aggregate capability presence rather than exact evidence shape or location. Prefer one assertion comment per expected diagnostic and keep fields as specific as needed for precision. A case with configured rules and no assertions verifies that the selected tool produces no diagnostics.
