# glass-lint-core

`glass-lint-core` is the provider-neutral analysis engine. It accepts owned
JavaScript or TypeScript sources, a validated rule catalog, an explicit host
environment, and typed module resolutions. It produces bounded,
deterministically ordered reports without filesystem access.

## Define a catalog

Rules use local IDs; `RuleCatalog` adds the provider namespace:

```rust
use glass_lint_core::rules::{Category, Confidence, QueryDecl, Rule, Severity};
use glass_lint_core::{Environment, Linter, LinterConfig, RuleCatalog};

let rule = Rule::builder("network.request")
    .description("Makes a network request")
    .category(Category::new("network")?)
    .severity(Severity::Warning)
    .confidence(Confidence::High)
    .query(QueryDecl::call_global("fetch")?)
    .build()?;

let mut environment = Environment::default();
environment.add_global("fetch")?;

let catalog = RuleCatalog::new("example", vec![rule])?;
let linter = Linter::new(LinterConfig::new(
    vec![catalog],
    environment,
))?;
let report = linter.lint_snippet("fetch('/data');", "main.js")?;

assert_eq!(report.files[0].findings[0].rule_id.as_str(), "example:network.request");
```

`LinterConfig` accepts one or more catalogs, one complete host environment, a
`RuleSelection`, and analysis limits. `Linter::new` validates and compiles the
complete configuration once. Selection baselines and ordered exact or glob
overrides are resolved during construction.

## Matching

Strict query predicates prove identity or connected semantics instead of matching
spelling alone. The public builder API covers:

- global and module-provenance calls and constructors;
- rooted, returned-object, instance, and module member behavior;
- imports and parsed literals;
- static value and rooted-expression argument constraints; and
- bounded object lifecycles and connected source-to-sink flow.
- multi-event object flow with `LifecycleCompletion::all_sinks`, which
  requires several later sinks to consume the same tracked object.

APIs named `heuristic` deliberately use weaker syntactic evidence.
Unsupported, ambiguous, dynamic, or exhausted analysis does not become a
strict match.

## Environments and projects

`Environment::default()` contains host-independent ECMAScript globals.
Providers add browser, Node.js, Electron, or application bindings. Use
`add_global_object` for an unrestricted current-realm alias and
`add_global_object_with_members` for a restricted host object.

For one source, call `Linter::lint_snippet`. For an in-memory project, use
`Linter::lint_project` or `ProjectCollection` with owned `SourceFile` values and
typed `ResolutionResult` records. Filesystem discovery and module resolution
belong in `glass-lint-project`.

`AnalysisReport` contains sorted file reports, structured diagnostics,
operation counts (including bounded alternative, trace, coalescing, and
fixed-point metrics), and `ReportCompletion`. Locations use one-based Unicode
display columns. Each finding includes `certainty` (`definite` or `possible`)
and bounded, correlated evidence traces. `definite` means the strict proof
holds on every modeled path reaching the occurrence; `possible` means a
complete strict witness exists on at least one path. TypeScript is normalized
but not type-checked. Sources larger than 8 MiB are rejected.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the internal pipeline and the
workspace [testing guide](../TESTING.md) for query coverage.

## Query declarations

The compiled example [`examples/query_declarations.rs`](examples/query_declarations.rs)
covers the supported authoring surface: ordinary and constrained events,
alternatives, same-event conjunctions, returned-object and instance member
queries, lifecycle flow, and structured construction errors. Query identity,
argument indexes, collections, lifecycle stages, and evidence symbols are
validated before catalog compilation; invalid input is returned as a typed
`QueryBuildError`.

`heuristic` identities intentionally provide weaker syntactic evidence than
strict global, rooted, or module identities. `QueryDecl::any` combines
independent complete alternatives, while `QueryDecl::all` is restricted to
constraints on one selected event. Bounded multi-event object correlation is
available through lifecycle conditions and `all_sinks`; unrelated events are
never joined by spelling alone. Nested `Any` inside `All` is an internal
compiler-test shape, not a public declaration route.
