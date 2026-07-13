# Testing strategy

Glass Lint tests behavior at several layers. Use the lowest layer that proves
the change, then add higher-level coverage when the behavior crosses package
or tool boundaries.

## Test layers

| Layer | Location | Purpose |
|---|---|---|
| Unit tests | Rust modules under `src/` | Local invariants, validation, normalization, and small algorithms |
| Core integration tests | `glass-lint-core/tests/` | Public matcher behavior, scope precision, semantic flow, and compact-source behavior |
| Rule fixtures | Beside provider rules | One rule's intended positives and adversarial negatives |
| End-to-end cases | `tests/e2e/` | Realistic snippets that exercise several rules and providers together |
| External comparisons | Harness adapters and `reports/` | Compare structured findings across tools |
| Profiling checks | Harness `profile` command | Throughput, slow files, determinism, and error totals on corpora |

Run all required layers with `make ci`. During development, prefer a targeted
command first:

```sh
cargo test -p glass-lint-core --test scope_precision
cargo test -p glass-lint-obsidian
cargo run -p glass-lint-cli --bin glass-lint-harness -- \
  verify glass-lint-obsidian/src/rules/network/request
```

## What matching tests must cover

A rule or matcher change needs a focused positive and an adversarial negative
for every relevant boundary:

- direct usage and supported call variants;
- ESM, CommonJS, namespace, destructured, and interop provenance;
- aliases before and after reassignment;
- lexical shadowing and local same-name lookalikes;
- static computed properties and dynamic property exclusions;
- supported static values and rejected dynamic values;
- connected source-to-sink flow and disconnected lookalikes;
- constructors, instances, callbacks, or returned-object lifecycles where
  applicable;
- minified or bundled shapes when transformations affect the behavior.

Tests should demonstrate the intended precision boundary, not merely increase
line coverage. Unknown or unsupported semantics should fail closed. Avoid
brittle wall-clock assertions and unordered snapshots.

## Writing a rule fixture

Each provider rule normally has two harness cases next to its `mod.rs`:

- `positive.js` contains supported forms that must report.
- `negative.js` contains shadowed, reassigned, local, dynamic, or near-name
  forms that must not report.

Configuration directives must appear in the leading comment block, before
executable code:

```js
// @case description Detects a proven Obsidian request import
// @case tags network,obsidian
// @tool glass-lint rules=obsidian:network.request

import { requestUrl } from "obsidian";

requestUrl("https://example.com");
// @expect-error-after glass-lint rule=obsidian:network.request message_id=detected
```

The default case ID is the path below the suite root without `.js`. Override
metadata only when useful:

```js
// @case id network/import-alias
// @case description Detects an aliased request import
// @case tags network,alias
// @case filename main.js
// @case language javascript
```

JavaScript is currently the only supported harness language.

### Tool configuration

Only tools named by `@tool` run for a case. Configure focused rules with a
comma-separated list:

```js
// @tool glass-lint rules=js:network.request,js:network.url-construction
```

The built-in adapter also accepts `config=heuristic`, which runs the complete
JavaScript and Obsidian catalogs:

```js
// @tool glass-lint config=heuristic
```

Do not combine `rules=` and `config=` in one tool directive. External adapters
define their own accepted configurations and rule IDs.

### Diagnostic assertions

Put an assertion immediately before, on, or after the source line it describes:

```js
// Applies to the next line.
// @expect-error glass-lint rule=js:network.request
fetch("/before");

// Applies to this line.
fetch("/inline"); // @expect-error glass-lint rule=js:network.request

// Applies to the previous code line.
fetch("/after");
// @expect-error-after glass-lint rule=js:network.request
```

Supported fields are:

| Field | Meaning |
|---|---|
| `rule` | Required namespaced rule ID |
| `message_id` | Stable diagnostic message identifier |
| `severity` | `info`, `warning`, or `error` |
| `count` | Expected number of matching diagnostics, or `any` |
| `line` | One-based source line, or `any` |
| `column` | One-based display column, or `any` |
| `message` | Exact message value without whitespace |

The default is `count=1` on the adjacent source line. Prefer exact locations
and one assertion per diagnostic. Use `count=any`, `line=any`, or
`column=any` only when testing aggregate capability presence rather than the
shape of evidence.

Use `@expect-no-error` or `@expect-no-error-after` for a specific forbidden
diagnostic while allowing other expected findings in the same case:

```js
// @tool glass-lint rules=js:network.request

fetch("/remote"); // @expect-error glass-lint rule=js:network.request
function local(fetch) {
  fetch("/local"); // @expect-no-error glass-lint rule=js:network.request
}
```

A configured case with no assertions verifies that the tool emits no
diagnostics.

## Choosing the right fixture location

- Put one rule's semantic contract in its colocated `positive.js` and
  `negative.js` files.
- Put reusable public matcher semantics in `glass-lint-core/tests/`.
- Put realistic workflows or interactions among rules in `tests/e2e/`.
- Do not duplicate a large case at several layers; extract the smallest
  regression at the layer that owns the behavior.

## Harness commands

Verify cases and fail on mismatches:

```sh
cargo run -p glass-lint-cli --bin glass-lint-harness -- verify tests/e2e
```

Render results without verification:

```sh
cargo run -p glass-lint-cli --bin glass-lint-harness -- \
  report tests/e2e --format markdown
cargo run -p glass-lint-cli --bin glass-lint-harness -- \
  report tests/e2e --format json
```

Register an external adapter with `--adapter NAME=COMMAND`. The harness sends
one JSON request to a fresh process for each case and expects one JSON response
using `ADAPTER_PROTOCOL_VERSION`; see `glass-lint-harness` for the public Rust
request and response types.

## Completion checklist

Before finishing a behavior change, confirm that:

1. positives report at deterministic, correct locations;
2. shadowed, reassigned, local, and dynamic lookalikes do not report;
3. the narrow fixture or integration test passes;
4. `make ci` passes; and
5. documentation reflects any changed boundary.
