# Glass Lint

Glass Lint detects JavaScript and TypeScript capabilities in source files and
projects. Its rules cover browser, Node.js, Electron, and Obsidian APIs while
distinguishing real references from shadowed names, local lookalikes, and
invalidated aliases.

Strict rules require lexical identity, module provenance, static values, or
connected value flow. The broader `heuristic` profile adds syntactic and
literal-based discovery.

> [!NOTE]
> Glass Lint is under active development. Rust APIs, report schemas, and rule
> IDs may change before a stable release.

## Supported input

- JavaScript, JSX, and ordinary TypeScript in `.js`, `.cjs`, `.mjs`, `.ts`,
  `.cts`, and `.mts` files
- Single-file analysis and bounded project analysis with import resolution
- Deterministic human-readable and JSON reports

TypeScript is normalized but not type-checked. TSX and declaration files are
not supported. A source file may be at most 8 MiB.

## Example

Glass Lint can connect a value's creation, configuration, aliases, and eventual
use. This reports `js:dom.remote-resource` because the remote URL reaches a DOM
insertion sink:

```js
const script = document.createElement("script");
script.src = "https://example.com/plugin.js";
const resource = script;
document.head.appendChild(resource);
```

A local URL, disconnected element, shadowed `document`, or reassigned alias
does not satisfy the strict rule.

## Get started

Build from source with a recent Rust toolchain:

```sh
cargo build --workspace
cargo run -p glass-lint-cli --bin glass-lint -- rules
cargo run -p glass-lint-cli --bin glass-lint -- check path/to/project
```

`check` accepts an entry file, directory, or `tsconfig.json` and analyzes the
admitted internal module graph. Use `snippet` for exactly one file:

```sh
cargo run -p glass-lint-cli --bin glass-lint -- snippet path/to/main.js
```

The CLI defaults to the combined Obsidian and JavaScript catalogs with the
complete `heuristic` profile. For high-confidence rules:

```sh
cargo run -p glass-lint-cli --bin glass-lint -- \
  --config-json '{"version":2,"cli":{"profile":"recommended"}}' \
  check path/to/project
```

See the [`glass-lint-cli` README](glass-lint-cli/) for configuration, output,
and exit statuses.

## Rust API

Provider crates expose catalogs and complete host environments; core constructs
the linter:

```rust
let linter = glass_lint_core::Linter::new(glass_lint_core::LinterConfig::new(
    vec![glass_lint_obsidian::catalog()], glass_lint_obsidian::environment(),
))?;
let report = linter.lint_snippet(source, "main.js")?;
```

Use [`glass-lint-core`](glass-lint-core/) to define custom catalogs,
[`glass-lint-project`](glass-lint-project/) to load filesystem projects, or a
provider crate for built-in policy. Reports contain sorted findings, bounded
evidence, structured diagnostics, and an explicit completion state.

## Repository documentation

- [Workspace architecture](ARCHITECTURE.md) defines crate dependencies and
  ownership.
- [Contributing](CONTRIBUTING.md) lists the development workflow and commands.
- [Testing](TESTING.md) defines test placement and fixture syntax.
- Each crate has a usage README and an `ARCHITECTURE.md` for its internal
  design.

## License

Glass Lint is licensed under the Mozilla Public License 2.0 only (`MPL-2.0`).

Parts of this project were developed with AI assistance.
