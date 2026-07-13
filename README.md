# Glass Lint

Glass Lint is a precision-first JavaScript analysis engine for identifying API
and platform capabilities in source files and bundled code. The repository
includes rule providers for JavaScript runtimes and Obsidian plugins, a JSON
CLI, and a snippet-based conformance harness.

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

## Get started

Glass Lint currently builds from source. Install a recent Rust toolchain, clone
the repository, and run:

```sh
cargo build --workspace
cargo run -p glass-lint-cli --bin glass-lint -- rules
cargo run -p glass-lint-cli --bin glass-lint -- check path/to/main.js
```

`check` accepts either one JavaScript file or a directory, which is searched
recursively for `.js` files. Results are emitted as formatted JSON.

Use the generic JavaScript provider or enable the broader profile as needed:

```sh
cargo run -p glass-lint-cli --bin glass-lint -- \
  --provider js check path/to/bundle.js

cargo run -p glass-lint-cli --bin glass-lint -- \
  check path/to/main.js --profile heuristic

cargo run -p glass-lint-cli --bin glass-lint -- \
  check path/to/main.js --rule obsidian:network.request
```

The global `--provider` option accepts `obsidian` (the default) or `js`.
Explicit `--rule` values must belong to that provider. Run `rules` with the
same provider to inspect rule metadata.

### Exit status

The CLI exits with status `0` when analysis succeeds without a finding at or
above `--fail-on`, `1` when the configured threshold is met or parsing fails,
and `2` for invalid arguments or operational errors. The default threshold is
`error`; accepted values are `info`, `warning`, `error`, and `never`.

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
| [`glass-lint-cli/`](glass-lint-cli/) | `glass-lint` and `glass-lint-harness` binaries |
| [`adapters/`](adapters/) | External harness integrations |
| [`tests/e2e/`](tests/e2e/) | Cross-rule, end-to-end JavaScript scenarios |

For implementation details, see [ARCHITECTURE.md](ARCHITECTURE.md). To build,
test, profile, or contribute, start with [CONTRIBUTING.md](CONTRIBUTING.md) and
[TESTING.md](TESTING.md). The current development backlog is tracked in
[plan.md](plan.md).

## License

Glass Lint is licensed under the Mozilla Public License 2.0 only (`MPL-2.0`).
