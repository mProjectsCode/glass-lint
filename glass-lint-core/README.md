# glass-lint-core

`glass-lint-core` is the provider-neutral JavaScript analysis engine behind
Glass Lint. It parses JavaScript and JSX, builds shared lexical and semantic
indexes, executes validated declarative matchers, and returns deterministic
structured reports.

This crate intentionally contains no Obsidian or JavaScript-platform rule
policy. Provider crates supply catalogs of rules through the public extension
API.

## Basic usage

Build a rule with a local ID, then place it in a namespaced catalog:

```rust
use glass_lint_core::{Environment, Linter, RuleCatalog};
use glass_lint_core::rules::{CallMatcher, Confidence, Rule, Severity};

let rule = Rule::builder("network.request")
    .label("Makes a network request")
    .category("network")
    .severity(Severity::Warning)
    .confidence(Confidence::High)
    .matcher(CallMatcher::global("fetch"))
    .build()
    .expect("valid rule");

let mut environment = Environment::default();
environment.add_global("fetch").expect("valid global");
environment
    .add_global_object("window")
    .expect("valid global object");

let catalog = RuleCatalog::with_environment("example", vec![rule], environment)
    .expect("valid catalog");
let report = Linter::new(catalog).lint("fetch('/data');", "main.js");

assert_eq!(report.findings[0].rule_id.as_str(), "example:network.request");
```

`Linter::new` enables every rule in a catalog. Use `Linter::with_rules` with
parsed `RuleId` values to enable a validated subset.

## Host environments

`Environment::default()` contains only host-independent ECMAScript globals,
including `Math`, `Function`, `eval`, and the standard `globalThis` global
object. Strict global and rooted provenance does not treat arbitrary unbound
names as host APIs.

Providers add their own global bindings and global-object aliases before
constructing a catalog. Environments are additive: call `add_global`,
`add_globals`, `add_global_object`, or `add_global_object_with_members`, or
merge another configuration with `extend`. An unrestricted global-object
alias maps a direct property to global callable identity when that property is
also a configured global binding. A restricted global object only promotes
its explicitly listed members, which is useful for foreign realms that do not
inherit current-realm host injections.

For example, an Obsidian-oriented caller can extend a browser/Electron
environment with a restricted `activeWindow`, then pass it to a provider's
`recommended_linter_with_environment` or `heuristic_linter_with_environment`
constructor.

```rust
environment
    .add_global_object_with_members("activeWindow", ["eval", "fetch"])
    .expect("valid global object and members");
```

## Matcher families

The `glass_lint_core::rules` module exports builders for:

- global and module-provenance calls, including proven global-object access,
  aliases, bind, call, and statically unpackable apply forms;
- rooted, module, and explicitly heuristic member calls and reads;
- global, module, and heuristic constructors and classes;
- imports and parsed string literals;
- static string, object-key, and rooted-expression argument constraints;
- returned-object and instance-member behavior; and
- bounded object-lifecycle flows declared with `ObjectFlowMatcher`, explicit
  `FlowCondition`, and `FlowCompletion` values.

Argument constraints use one vocabulary: `.arg(index, ValueMatcher::...)`.
`ValueMatcher::any_value()` intentionally accepts dynamic values, while
`ValueMatcher::static_string()` requires a proven static string.

Prefer provenance-aware matchers. The APIs containing `heuristic` in their
names deliberately require callers to opt in to weaker syntactic matching.
Rules are normalized and validated when built, then compiled once when a
catalog is constructed.

## Reports and limits

`Linter::lint` accepts one source string and filename. A `LintReport` contains
schema and tool versions, sorted findings, bounded evidence, and parse
diagnostics. Finding locations use one-based Unicode display columns.

JavaScript sources larger than `MAX_SOURCE_BYTES` (8 MiB) return a structured
parse diagnostic. Parsing stops after the first parser diagnostic. Each rule
retains at most `Rule::EVIDENCE_LIMIT` (16) source occurrences so report size
remains bounded.

See the repository [architecture](../ARCHITECTURE.md) for the internal
pipeline and [testing guide](../TESTING.md) for matcher test expectations.
