# Core architecture

`glass-lint-core` is the provider-neutral semantic engine. It depends on no
other workspace crate and performs no filesystem access.

## Pipeline

```text
source + language + environment + limits
  -> parse and TypeScript normalization
  -> scopes, bindings, provenance, and semantic facts
  -> immutable local artifact
  -> module interfaces and bounded flow summaries
  -> project linking
  -> compiled matcher queries
  -> deterministic AnalysisReport
```

Parsing and fact construction happen once per admitted source. Rules query the
shared artifact; enabling a rule must not add an AST traversal or a separate
semantic model.

## Internal ownership

- `parse` and `analysis/syntax` contain the private SWC-backed frontend.
- `analysis/scope` owns bindings, shadowing, reassignment, and provenance.
- `analysis/facts` owns the matcher-independent event stream.
- `analysis/resolution` and `analysis/value` own identity and static-value
  resolution.
- `analysis/flow` owns bounded local and cross-call flow.
- `analysis/matching` owns occurrence indexes and query execution.
- `analysis/project` links module identities and cross-file effects.
- `api/rule` validates declarative rule definitions.
- `api/compiler` compiles catalogs into immutable matcher plans.
- `api/rule` exposes validated package-boundary module patterns and bounded
  sink-associated static-string predicates; exact module identities remain
  distinct from package-root patterns.
- `lint` selects rules and constructs findings.
- `project` exposes owned inputs, typed resolutions, sessions, and reports.

SWC types stay inside local analysis. Retained artifacts, project linking,
provider crates, and public reports use core domain types.

## Runtime and cache boundary

`Linter` owns a compiled catalog, selected rules, analysis limits, and a shared
bounded local-artifact cache. Cloned linters and configuration changes reuse
the cache when the source, language, environment, engine version, and all
artifact-affecting limits match. Rule selection is not part of the artifact
identity.

The cache is in-memory only. Parsing does not run while its lock is held;
poisoning is a miss; parse failures are not cached. Cached artifacts contain
no path-specific source context and cannot change report content or operation
counts.

## Project boundary

`AnalysisSession` accepts owned `SourceFile` values and typed
`ResolutionResult` records. It never discovers files or resolves modules.
After admission, core links imports, exports, re-exports, identities, and
supported call flow, then runs matchers over the linked model.

Ambiguous exports, missing resolutions, unsupported module shapes, and
exhausted budgets remain unknown. Findings stay with the file containing the
primary event.

## Invariants

- Core contains no provider names, APIs, profiles, categories, manifests, or
  disclosure policy.
- Strict matches require proven identity, provenance, static values, or
  connected flow at the use position.
- Shadowing, reassignment, ambiguity, unsupported semantics, and exhausted
  budgets fail closed.
- Work, intermediate state, evidence, and output are bounded.
- Files, findings, evidence, diagnostics, and operation counts are
  deterministic.
- Parser, scope, fact, compiler, cache, and budget internals remain private.

Core stays one crate while these layers share an evolving private semantic
model. A split requires a stable, independently owned contract, acyclic
dependency direction, and measured build-time benefit.
