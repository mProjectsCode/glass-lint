# Core architecture

`glass-lint-core` is the provider-neutral semantic engine. It depends on
[`glass-lint-datastructures`](../glass-lint-datastructures/ARCHITECTURE.md) for
bounded storage primitives and performs no filesystem access.

## Pipeline

```text
source + language + environment + limits
  -> parse and TypeScript normalization
  -> scopes, bindings, provenance, and semantic facts
  -> immutable local artifact
  -> module interfaces and bounded flow summaries
  -> project linking
  -> compiled query plans
  -> deterministic AnalysisReport
```

Parsing and fact construction happen once per accepted source. Rules query the
shared artifact; enabling a rule must not add an AST traversal or a separate
semantic model.

## Internal ownership

- `parse` and `analysis/syntax` contain the private SWC-backed frontend.
- `analysis/semantic` owns the parser-to-artifact semantic analysis boundary,
  bounded analysis budget, and completion status.
- `analysis/scope` owns bindings, shadowing, reassignment, and provenance.
- `analysis/facts` owns the query-independent event stream.
- `analysis/resolution` and `analysis/value` own identity and static-value
  resolution.
- `analysis/flow` owns bounded local and cross-call flow.
- `analysis/matching` owns occurrence indexes and query execution.
- `analysis/project` links module identities and cross-file effects.
- `api/rule` validates declarative rule definitions.
- `api/compiler` compiles catalogs into immutable query plans.
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

Batch linting accepts owned `SourceFile` values and runs each as an independent
one-file project. One dedicated Rayon pool is created per batch; the pool
executes complete `lint_source` operations while the caller retains and
advances the input iterator. Submitted inputs, including running jobs and
completed results waiting for input ordering, are bounded by
`max_in_flight`. Results are yielded in input order, and dropping the iterator
cancels queued work without consuming inputs beyond the submitted window.

## Project boundary

`ProjectSession` accepts owned `SourceFile` values and produces an
`AnalyzedSource` for each completed local semantic analysis. Consuming transitions to
`LocallyAnalyzedProject` and `ResolvedProject` ensure that linking and
matching can run only after authored resolver outcomes have been validated.
Core never discovers files or resolves modules.

Ambiguous exports, missing resolutions, unsupported module shapes, and
exhausted budgets remain unknown. A complete strict witness can still produce
a `Possible` finding when an independent alternative is unknown; those
alternatives prevent `Definite`. Findings stay with the file containing the
primary event.

## Invariants

- Core contains no provider names, APIs, profiles, categories, or
  manifests.
- Strict matches require proven identity, provenance, static values, or
  connected flow at the use position on one complete modeled path.
- Shadowing, reassignment, ambiguity, unsupported semantics, and exhausted
  alternatives cannot establish a witness. They prevent definite path
  coverage but do not erase an independent complete possible witness.
- Joins retain bounded correlated alternatives; they never combine aliases,
  requirements, sources, and sinks from incompatible paths.
- Findings distinguish `Definite` and `Possible` path coverage. Incomplete
  analysis never claims `Definite`.
- Work, intermediate state, evidence, and output are bounded.
- Files, findings, evidence, diagnostics, and operation counts are
  deterministic.
- Parser, scope, fact, compiler, cache, and budget internals remain private.
- `NameId` values are opaque and artifact-local; they may be compared only
  with the `NameTable` retained by the same semantic artifact. Textual
  ordering and cross-artifact/project interfaces continue to use strings.

Core stays one crate while these layers share an evolving private semantic
model. A split requires a stable, independently owned contract, acyclic
dependency direction, and measured build-time benefit.

## Adding query capabilities

Define the provider-neutral relation and certainty rule in `api/rule/query`
first, including unknown and incomplete alternatives. Add construction and
validation errors before lowering. Normalize to a canonical form, compute exact
preparation requirements, and add a specialized physical root only when an
existing root cannot express the access path.

New relations require positives and adversarial negatives for shadowing,
lookalikes, reassignment, dynamic values, incompatible joins, unsupported
escape, ambiguity, and budget exhaustion. Assert evidence order and certainty.
Use `CompiledMatcherPlan::plan_explanation` in compiler and profiling tests;
it is deterministic and does not expose artifact-local IDs.

The public-surface audit keeps compiler IR, physical roots, fact IDs, caches,
and executor storage private. Public query declarations expose only validated
semantic constructors, while `public_surface.rs` guards that callers can
build and use rules without engine storage access.
