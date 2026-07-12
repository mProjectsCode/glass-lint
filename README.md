# Glass Lint

Glass Lint is a general JavaScript lint engine, an Obsidian rule pack, and a snippet-first conformance harness.

```sh
cargo run -p glass-lint-cli --bin glass-lint -- rules
cargo run -p glass-lint-cli --bin glass-lint -- check main.js
cargo run -p glass-lint-cli --bin glass-lint -- --provider js check main.js
cargo run -p glass-lint-cli --bin glass-lint-harness -- verify tests/e2e
```

## Folder profiling

The harness can profile arbitrary JavaScript files without case metadata.
Run it on one subfolder of a large production corpus:

    cargo run -p glass-lint-cli --bin glass-lint-harness -- profile --path /path/to/a/plugin/subfolder --provider obsidian --profile recommended --quiet

Use repeated --path options for multiple roots. Discovery is recursive,
deterministic, and does not follow symlinks. --include and --exclude use
glob::Pattern syntax with slash-separated paths. Patterns without a slash
also match basenames. --sample N --seed S selects a deterministic sample
after filtering. --warm-up N, --repeat N, --workers N, and
--continue-on-error control execution; the default is one worker.

The selected sources are read into memory before the measured phase. Per-file
timings and lint wall time cover only Linter::lint calls; setup and total
process time are reported separately. This keeps file reads and decoding out
of the useful timings.

Install Samply, then run:

    make profile PROFILE_PATH=/path/to/a/plugin/subfolder PROFILE_ARGS="--quiet --sample 20 --seed 20260712"

The Make target builds the release harness with debug symbols and invokes
samply record. Its default corpus is the small tests/e2e directory. Point
PROFILE_PATH at one subfolder of a large production corpus rather than the
entire release tree.

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

Core analysis is precision-first and bounded: strict matchers require lexical
and provenance evidence, dynamic/unsupported semantics fail closed, source
files over 8 MiB receive a structured parse diagnostic, and each rule keeps at
most 16 source occurrences in deterministic order. `Evidence` entries include
the first matching range and source snippet; report finding ranges are the
outermost non-contained matching spans.

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
