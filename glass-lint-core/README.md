# glass-lint-core

`glass-lint-core` is the provider-neutral analysis engine. It parses one source
file once, builds shared lexical and semantic facts, runs validated matchers,
and produces deterministic reports. It also owns the bounded cross-file model
used to link imports, exports, and supported call flow.

The crate contains no host-specific rule policy. Callers provide a catalog of
rules and an explicit host environment.

## Analyze one file

Rules use local IDs inside a namespaced `RuleCatalog`:

```rust
use glass_lint_core::rules::{CallMatcher, Confidence, Rule, Severity};
use glass_lint_core::{Environment, Linter, RuleCatalog};

let rule = Rule::builder("network.request")
    .label("Makes a network request")
    .category("network")
    .severity(Severity::Warning)
    .confidence(Confidence::High)
    .matcher(CallMatcher::global("fetch"))
    .build()?;

let mut environment = Environment::default();
environment.add_global("fetch")?;

let catalog = RuleCatalog::with_environment("example", vec![rule], environment)?;
let report = Linter::new(catalog).lint("fetch('/data');", "main.js");

assert_eq!(report.findings[0].rule_id.as_str(), "example:network.request");
```

`Linter::new` enables every rule in a catalog. `Linter::with_rules` enables an
exact validated set of `RuleId` values, and `Linter::with_confidence` selects a
confidence level. `Linter::combine_with_environment` combines existing
linters into one analysis pass while preserving their enabled rule sets.

## Matching model

Strict matchers prove what a value refers to instead of matching spelling
alone. The public rule API covers:

- global and module-provenance calls and constructors;
- rooted, module, returned-object, and instance member behavior;
- imports and parsed string literals;
- static string, object-key, and rooted-expression arguments; and
- bounded object lifecycles and connected source-to-sink flow.

Argument constraints use `.arg(index, ValueMatcher::...)`.
`ValueMatcher::any_value()` accepts a dynamic value;
`ValueMatcher::static_string()` requires a proven static string. APIs with
`heuristic` in their name deliberately opt into weaker syntactic evidence.

Rules are normalized and validated when built, then compiled once with their
catalog. Unsupported, ambiguous, dynamic, or budget-exhausted analysis does not
become a strict match.

## Host environments

`Environment::default()` includes host-independent ECMAScript globals such as
`Math`, `Function`, and `eval`. Add runtime bindings explicitly with
`add_global` or `add_globals`.

Global-object aliases need a little more care:

- `add_global_object` models an unrestricted alias of the current global
  object.
- `add_global_object_with_members` exposes only listed members, which is useful
  for another realm or a partially known host object.
- `extend` merges another environment configuration.

Unconfigured unbound names are not treated as host APIs.

## Cross-file analysis

`ProjectSession` accepts owned `SourceFile` values, exposes typed
`ResolutionRequest` records, and consumes `ResolutionResult` values supplied by
the caller. It never performs filesystem access or module resolution itself.
Once resolutions are supplied, it links the supported module graph and returns
a `ProjectReport` with deterministic findings, diagnostics, completion state,
and operation counts.

## Reports and limits

`Linter::lint` selects TypeScript for `.ts`, `.cts`, and `.mts`, and JavaScript
for `.js`, `.cjs`, and `.mjs`. TypeScript is normalized with fixed settings; it
is not type-checked or configured from `tsconfig.json`.

`LintReport` contains versioned, sorted findings and structured parse
diagnostics. Locations use one-based Unicode display columns. Evidence and
output are bounded; sources larger than `MAX_SOURCE_BYTES` (8 MiB) return a
parse diagnostic rather than being analyzed.

`PrettyReport`, `PrettyReports`, and `PrettyOptions` render bounded human
output without changing the structured report. `CoreConfig` applies resource
limits and exact rule selection to an existing linter.

See the repository [architecture](../ARCHITECTURE.md) for the internal pipeline
and [testing guide](../TESTING.md) for matcher test expectations.
