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
use glass_lint_core::{Linter, RuleCatalog};
use glass_lint_core::rules::{Confidence, Matcher, Rule, Severity};

let rule = Rule::builder("network.request")
    .label("Makes a network request")
    .category("network")
    .severity(Severity::Warning)
    .confidence(Confidence::High)
    .matcher(Matcher::global_call("fetch"))
    .build()
    .expect("valid rule");

let catalog = RuleCatalog::new("example", vec![rule])
    .expect("valid catalog");
let report = Linter::new(catalog).lint("fetch('/data');", "main.js");

assert_eq!(report.findings[0].rule_id.as_str(), "example:network.request");
```

`Linter::new` enables every rule in a catalog. Use `Linter::with_rules` with
parsed `RuleId` values to enable a validated subset.

## Matcher families

The `glass_lint_core::rules` module exports builders for:

- global and module-provenance calls;
- rooted, module, and explicitly heuristic member calls and reads;
- global, module, and heuristic constructors and classes;
- imports and parsed string literals;
- static string, object-key, and rooted-expression argument constraints;
- returned-object and instance-member behavior; and
- bounded, connected value-flow requirements and sinks.

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
