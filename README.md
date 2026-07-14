# Glass Lint

Glass Lint is a precision-first JavaScript analysis engine for identifying API
and platform capabilities in source files and bundled code. The repository
includes rule providers for JavaScript runtimes and Obsidian plugins, a CLI,
and a snippet-based conformance harness.

Glass Lint favors proven lexical identity, module provenance, and connected
value flow over name-only matching. The default `recommended` profiles include
high-confidence rules; broader discovery rules are available through the
opt-in `heuristic` profiles.

> [!NOTE]
> Glass Lint is under active development. Rust APIs, report schemas, and rule
> IDs may change before a stable release.

## What it analyzes

- JavaScript and JSX source, including minified bundles
- Browser, Node.js, and Electron capabilities through the `js` provider
- Obsidian API usage through the `obsidian` provider
- Imports, CommonJS loads, aliases, shadowing, reassignment, rooted member
  chains, static arguments, and bounded value flow

TypeScript syntax, automatic fixes, and suggestions are not currently
supported. A source file is limited to 8 MiB by the core parser.

## A small example

Glass Lint can follow an object from creation, through configuration and an
alias, to the sink that makes it interesting. It reports the positive flow:

```js
const script = document.createElement("script");
script.src = "https://example.com/plugin.js";
const resource = script;
document.head.appendChild(resource); // js:dom.remote-resource
```

The corresponding negative flow is deliberately similar, but does not match
`js:dom.remote-resource` because the URL is local:

```js
const script = document.createElement("script");
script.src = "/local.js";
document.head.appendChild(script); // no js:dom.remote-resource finding
```

With the full JavaScript catalog, the same snippet still produces
`js:dynamic-code.script-injection` though.

It also recognizes contrived dynamic-code paths, including a configured
foreign-realm `eval` and an async function constructor reached through the
prototype chain:

```js
activeWindow.eval(source); // js:dynamic-code.eval

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
const run = new AsyncFunction(`return (${source})`); // js:dynamic-code.eval
```

These examples correspond to the [remote-resource fixtures](glass-lint-js/src/rules/browser/remote_resource/)
and the [executable-code-blocks e2e case](tests/e2e/render-executable-code-blocks.js).

## Get started

Glass Lint currently builds from source. With a recent Rust toolchain installed,
clone the repository and run:

```sh
cargo build --workspace
cargo run -p glass-lint-cli --bin glass-lint -- rules
cargo run -p glass-lint-cli --bin glass-lint -- check path/to/main.js
```

`check` accepts either one JavaScript file or a directory, which is searched
recursively for `.js` files. Results default to bounded human-readable output;
rules are grouped across the input and their evidence is sorted by file and
source location while retaining copyable `path:line:column` values. JSON output
is selected through the versioned configuration schema.

The default Obsidian provider includes both generic JavaScript and
Obsidian-specific rules in one analysis pass. Use the standalone JavaScript
provider or enable the broader profile as needed:

```sh
cargo run -p glass-lint-cli --bin glass-lint -- \
  --config-json '{"version":1,"cli":{"provider":"js"}}' check path/to/bundle.js

cargo run -p glass-lint-cli --bin glass-lint -- \
  --config-json '{"version":1,"cli":{"profile":"heuristic"}}' check path/to/main.js

cargo run -p glass-lint-cli --bin glass-lint -- \
  --config glass-lint.toml check path/to/main.js

cargo run -p glass-lint-cli --bin glass-lint -- \
  --config-json '{"version":1,"core":{"rules":["obsidian:network.request"]}}' check path/to/main.js
```

Configuration contains `version = 1`, `[core]` exact rule selection, and
`[cli]` provider/profile, limits, failure threshold, output, verbosity, and
pretty width. It is loaded from `--config`, `--config-json`, or the current
directory's `glass-lint.toml`/`glass-lint.json`.

### Exit status

The CLI exits with status `0` when analysis succeeds without a finding at or
above the configured `cli.fail_on` threshold, `1` when that threshold is met
or parsing fails, and `2` for invalid arguments or operational errors. The
default threshold is `error`; accepted values are `info`, `warning`, `error`,
and `never`.

## Use as a Rust library

Provider crates expose ready-to-use linters:

```rust
let report = glass_lint_obsidian::recommended_linter()
    .lint(source, "main.js");
```

Select individual rules from a provider catalog when you need a focused
analysis:

```rust
use glass_lint_core::{Linter, RuleId};

let provider = glass_lint_obsidian::heuristic_linter();
let selected = [RuleId::parse("obsidian:network.request")?];
let linter = Linter::with_rules(provider.catalog().clone(), selected)?;
let report = linter.lint(source, "main.js");
```

Reports contain deterministic findings, one-based source locations, bounded
evidence, and structured parse diagnostics. Rule IDs use `provider:name`, for
example `js:network.request` and `obsidian:network.request`.

## Repository guide

| Path | Purpose |
|---|---|
| [`glass-lint-core/`](glass-lint-core/) | Provider-neutral parser, semantic analysis, matcher API, and report model |
| [`glass-lint-js/`](glass-lint-js/) | JavaScript, browser, Node.js, and Electron rules |
| [`glass-lint-obsidian/`](glass-lint-obsidian/) | Obsidian rules, profiles, and disclosure mappings |
| [`glass-lint-harness/`](glass-lint-harness/) | Reusable conformance-case runner and profiling library |
| [`glass-lint-cli/`](glass-lint-cli/) | `glass-lint` binary |
| [`glass-lint-harness-cli/`](glass-lint-harness-cli/) | Harness executable |
| [`adapters/`](adapters/) | External harness integrations |
| [`tests/e2e/`](tests/e2e/) | Cross-rule, end-to-end JavaScript scenarios |

For implementation details, see [ARCHITECTURE.md](ARCHITECTURE.md). To build,
test, profile, or contribute, start with [CONTRIBUTING.md](CONTRIBUTING.md) and
[TESTING.md](TESTING.md).

## AI Assistance Notice

Parts of this project were vibe coded with AI assistance.

## License

Glass Lint is licensed under the Mozilla Public License 2.0 only (`MPL-2.0`).
