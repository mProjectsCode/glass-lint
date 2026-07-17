# Glass Lint

Glass Lint finds JavaScript and TypeScript capabilities in source files and
bundles. It can identify browser, Node.js, Electron, and Obsidian APIs while
distinguishing real references from shadowed names, local lookalikes, and
invalidated aliases.

The analyzer is precision-first: strict rules rely on lexical identity, module
provenance, static values, or connected value flow. Broader name- and
literal-based discovery is available through the `heuristic` profile.

> [!NOTE]
> Glass Lint is under active development. Rust APIs, report schemas, and rule
> IDs may change before a stable release.

## Supported Files and Languages

- JavaScript, JSX, and ordinary TypeScript in `.js`, `.cjs`, `.mjs`, `.ts`,
  `.cts`, and `.mts` files
- Browser, DOM, network, Node.js, Electron, cryptography, archive, and
  dynamic-code rules in the `js:` namespace
- Obsidian API and plugin-capability rules in the `obsidian:` namespace
- Single-file analysis and bounded project analysis with import resolution
- Deterministic human-readable and JSON reports

TypeScript is parsed and normalized, but not type-checked. TSX and declaration
files are not supported. A single source file may be at most 8 MiB.

## Example

Glass Lint follows values through supported aliases and assignments. This
remote script is reported because the created element, remote URL, and DOM sink
belong to one connected flow:

```js
const script = document.createElement("script");
script.src = "https://example.com/plugin.js";
const resource = script;
document.head.appendChild(resource); // js:dom.remote-resource
```

A local URL does not satisfy that rule:

```js
const script = document.createElement("script");
script.src = "/local.js";
document.head.appendChild(script); // no js:dom.remote-resource finding
```

With the complete JavaScript catalog, both snippets also produce
`js:dynamic-code.script-injection`, because that rule describes script
execution rather than remote resource loading.

## Get started

Glass Lint currently builds from source. With a recent Rust toolchain:

```sh
cargo build --workspace
cargo run -p glass-lint-cli --bin glass-lint -- rules
cargo run -p glass-lint-cli --bin glass-lint -- check path/to/main.js
```

`check` accepts an entry file, a directory, or an explicit `tsconfig.json`.
It discovers a bounded project, follows admitted internal imports, and excludes
dependencies, declarations, common output directories, and files outside the
project boundary. Use `snippet` when cross-file linking is not wanted:

```sh
cargo run -p glass-lint-cli --bin glass-lint -- snippet path/to/main.js
```

The CLI defaults to the Obsidian provider and the complete `heuristic` profile.
The Obsidian provider includes both `js:*` and `obsidian:*` rules. A practical
high-confidence invocation is:

```sh
cargo run -p glass-lint-cli --bin glass-lint -- \
  --config-json '{"version":2,"cli":{"profile":"recommended"}}' \
  check path/to/main.js
```

Select only the generic JavaScript provider or an exact set of rules when
needed:

```sh
cargo run -p glass-lint-cli --bin glass-lint -- \
  --config-json '{"version":2,"cli":{"provider":"js"}}' \
  check path/to/main.js

cargo run -p glass-lint-cli --bin glass-lint -- \
  --config-json '{"version":2,"core":{"rules":["obsidian:network.request"]}}' \
  check path/to/main.js
```

Configuration can come from `--config`, `--config-json`, or
`glass-lint.toml`/`glass-lint.json` in the current directory. See the
[`glass-lint-cli` README](glass-lint-cli/) for the schema, output behavior, and
exit statuses.

## Use as a Rust library

Provider crates expose ready-to-use linters:

```rust
let report = glass_lint_obsidian::recommended_linter()
    .lint(source, "main.js");
```

The report contains sorted findings, one-based locations, bounded evidence,
and structured diagnostics. Rule IDs are namespaced as `provider:name`, such as
`js:network.request` and `obsidian:network.request`.

Use [`glass-lint-core`](glass-lint-core/) to build custom rule catalogs,
[`glass-lint-project`](glass-lint-project/) to load filesystem projects, or a
provider crate for a ready-made catalog.

## Workspace guide

| Path | Responsibility |
|---|---|
| [`glass-lint-core/`](glass-lint-core/) | Provider-neutral parsing, semantics, matchers, project linking, and reports |
| [`glass-lint-project/`](glass-lint-project/) | Filesystem discovery, source loading, project boundaries, and module resolution |
| [`glass-lint-js/`](glass-lint-js/) | JavaScript, browser, Node.js, and Electron policy |
| [`glass-lint-obsidian/`](glass-lint-obsidian/) | Obsidian policy, profiles, and disclosures |
| [`glass-lint-cli/`](glass-lint-cli/) | The `glass-lint` command |
| [`glass-lint-harness/`](glass-lint-harness/) | Reusable conformance, adapter, report, and profiling library |
| [`glass-lint-harness-cli/`](glass-lint-harness-cli/) | The `glass-lint-harness` command |
| [`adapters/`](adapters/) | External harness adapters |
| [`tests/e2e/`](tests/e2e/) | Cross-rule end-to-end cases |

For design boundaries, see [ARCHITECTURE.md](ARCHITECTURE.md). For development
commands and test conventions, see [CONTRIBUTING.md](CONTRIBUTING.md) and
[TESTING.md](TESTING.md).

## AI assistance notice

Parts of this project were developed with AI assistance.

## License

Glass Lint is licensed under the Mozilla Public License 2.0 only (`MPL-2.0`).
