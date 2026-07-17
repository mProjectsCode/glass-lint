# Testing

Use the lowest test layer that proves a change. Add a higher layer only when
behavior crosses a crate or executable boundary.

## Test placement

| Layer | Location | Purpose |
|---|---|---|
| Unit | Rust modules under `src/` | Private invariants, validation, and small algorithms |
| Core integration | `glass-lint-core/tests/` | Public matcher, scope, provenance, flow, and report behavior |
| Project | `glass-lint-project/src/tests.rs` and `tests/projects/` | Discovery, boundaries, resolution, and multi-file behavior |
| Provider contract | Beside each rule | One rule's positives and adversarial negatives |
| End to end | `tests/e2e/` | Realistic cross-rule and cross-provider workflows |
| External comparison | Adapters and `reports/` | Descriptive output comparison across tools |

Do not copy a large case across layers. Extract the smallest regression at the
layer that owns the behavior.

## Required matching coverage

Cover each boundary relevant to the matcher:

- direct use and supported call forms;
- ESM, CommonJS, namespace, destructured, and interop provenance;
- aliases before and after reassignment;
- lexical shadowing and local same-name lookalikes;
- static computed properties and rejected dynamic properties;
- accepted static values and rejected dynamic values;
- connected and disconnected source-to-sink flow;
- constructors, returned objects, callbacks, and lifecycle stages; and
- minified or bundled shapes where transformation affects semantics.

Unknown, ambiguous, unsupported, dynamic, or budget-exhausted semantics must
fail closed. Assert deterministic rule IDs and exact locations. Avoid
wall-clock tests and unordered snapshots.

For cross-file flow, use a virtual project with explicit resolution records.
Assert the finding in the sink file, not merely the presence of the same rule
elsewhere.

## Rule fixtures

A provider rule normally has `positive.js` and `negative.js` beside `mod.rs`.
Only tools named in a leading `@tool` directive run:

```js
// @case description Detects a proven Obsidian request import
// @case tags network,obsidian
// @tool glass-lint rules=obsidian:network.request

import { requestUrl } from "obsidian";

requestUrl("https://example.com");
// @expect-error-after glass-lint rule=obsidian:network.request message_id=detected
```

The built-in adapter accepts either comma-separated `rules=` or
`config=heuristic`; do not combine them. Optional leading metadata includes
`@case id`, `description`, `tags`, `filename`, and `language`.

Expectations can target the next, current, or previous code line:

```js
// @expect-error glass-lint rule=js:network.request
fetch("/next");

fetch("/inline"); // @expect-error glass-lint rule=js:network.request

fetch("/previous");
// @expect-error-after glass-lint rule=js:network.request
```

Use `@expect-no-error` and `@expect-no-error-after` for a specific forbidden
diagnostic while allowing other expected findings. A configured case with no
expectations asserts that the tool emits no diagnostics.

Expectation fields are:

| Field | Meaning |
|---|---|
| `rule` | Required namespaced rule ID |
| `message_id` | Stable message identifier |
| `severity` | `info`, `warning`, or `error` |
| `count` | Expected count or `any` |
| `line` | One-based source line or `any` |
| `column` | One-based display column or `any` |
| `message` | Exact whitespace-free message value |

The default is one diagnostic on the adjacent source line. Use `any` only for
intentional aggregate assertions.

Supported fixture extensions are `.js`, `.cjs`, `.mjs`, `.ts`, `.cts`, and
`.mts`. The extension determines the language; TypeScript is not type-checked.

## Project fixtures

Multi-file cases live under `tests/projects/<name>/` with a `case.toml`.
Virtual projects list explicit `[[resolution]]` records. Filesystem projects
set `filesystem = true` to exercise discovery and Oxc resolution. Assertions
stay in the source file they describe.

Snippet-only external adapters are skipped deterministically for project
cases.

## Commands

Run a narrow test first:

```sh
cargo test -p glass-lint-core --test scope_precision
cargo test -p glass-lint-project
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  verify glass-lint-obsidian/src/rules/network/request
```

Render without verification:

```sh
cargo run -p glass-lint-harness-cli --bin glass-lint-harness -- \
  report tests/e2e --format markdown
```

External adapters use `--adapter NAME=COMMAND`. The harness starts a fresh
process per case, writes one versioned JSON request to stdin, and reads one
JSON response from stdout.

Before finishing a behavior change, run:

```sh
make ci
```
