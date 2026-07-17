# Architecture

Glass Lint separates provider-neutral JavaScript analysis from provider policy
and filesystem concerns. Core analyzes each source once, providers describe
what to detect, and front ends select rules and present results.

## Workspace boundaries

```text
glass-lint-cli ---------> glass-lint-project -----> glass-lint-core
       |                         |                        ^
       +-------------------------+------------------------+
       |                                                  |
       +----------------> provider crates ----------------+

glass-lint-harness-cli -> glass-lint-harness ------> the same layers
```

Dependencies point toward `glass-lint-core`. Each crate has one primary role:

- `glass-lint-core` owns parsing, semantic analysis, matcher execution, rule
  catalogs, and generic reports. It has no filesystem or provider knowledge.
- `glass-lint-project` owns discovery, source loading, project boundaries,
  `tsconfig` membership, and module resolution. It gives core owned sources
  and typed resolution results.
- `glass-lint-js` owns generic JavaScript, browser, Node.js, and Electron rules
  and host assumptions.
- `glass-lint-obsidian` owns Obsidian rules, profiles, disclosures, and the few
  provider-specific semantics that cannot be expressed by core matchers.
- `glass-lint-harness` owns fixture parsing, adapters, verification,
  comparison reports, and profiling.
- `glass-lint-cli` and `glass-lint-harness-cli` are thin executables. Reusable
  behavior belongs in the library crates.

Do not duplicate parsing, semantic models, match paths, or report types across
these boundaries. Move reusable semantics inward; keep policy outward.

## Core analysis

Each file follows one matcher-independent pipeline:

```text
authored source
  -> deterministic admission
  -> private parser adapter and zero-based coordinate normalization
  -> build scopes, bindings, and provenance
  -> emit semantic facts
  -> resolve identities, constants, aliases, and values
  -> immutable LocalArtifact
  -> shared indexes and flow summaries
  -> private compiled query execution
  -> canonical AnalysisReport construction
```

Parsing and fact construction happen once per file. Enabling a rule must not
add an AST traversal or alter the semantic model. Matchers query shared indexes
after analysis. Admitted files may be lowered in bounded parallel jobs; results
and operation counts are merged by normalized source identity. Project
linking remains deterministic and single-threaded until a measured need says
otherwise.

The core layers are:

- `analysis/syntax`: small AST-level naming, constant, and provenance helpers
- `analysis/scope`: lexical bindings, shadowing, and reassignment
- `analysis/facts`: the canonical matcher-independent event stream
- `analysis/resolution`: expression, call, and constant resolution
- `analysis/value`: stable value identities and property paths
- `analysis/flow`: bounded local and cross-call flow
- `analysis/matching`: occurrence indexes and evidence queries
- `api/rule`: validated public rule and matcher definitions
- `api/compiler`: immutable matcher plans
- `lint`: rule selection and report construction

The parser, AST, scope, fact, index, compiler, and budget modules are private
implementation details. Providers extend core through
`glass_lint_core::rules`, `RuleCatalog`, and `Linter` rather than building a
parallel analysis path. The retained local artifact contains no interior
mutation and is `Send + Sync`; construction-local caches do not cross the
artifact boundary.

### Artifact and cache boundary

`LocalArtifact` is keyed privately by source bytes, source language and
normalization mode, host `Environment`, semantic engine version, and all
`ResourceLimits` that affect retained state. Rule selection is not a key input.
The process-local cache is bounded, uses deterministic eviction, and
intentionally does not define a persistent serialization format; parse
failures are not cached, while exhausted successful artifacts retain their
typed status. A cache hit attaches only the current path-specific source
context.

### Query and report boundary

Validated provider matchers compile once per catalog into a private normalized
query. Query execution consumes fact and identity indexes and returns typed
evidence. Identity, event, subject, and `QueryConstraint` predicates are
evaluated by the same ordinary clause executor; there is no family-specific
argument execution path. Completion is represented by `ReportCompletion`, and
diagnostics remain typed records rather than string-based completion signals.

SWC positions are converted once during lowering into checked, zero-based,
half-open `ByteRange` values. `SourceLineIndex::try_range` is then used by
retained consumers such as findings, authored requests, and module/linking
diagnostics; invalid bounds or UTF-8 boundaries become incomplete analysis
rather than clamped or fabricated locations. `AnalysisReport` is source-free:
front ends retain source text only in presentation state. Reports combine
losslessly, preserving completion, file/project diagnostics, and operation
counters while rejecting incompatible schema or tool versions.

## Project analysis

Project analysis keeps filesystem policy and semantic proof separate:

1. `glass-lint-project` selects sources, enforces project and resource limits,
   and resolves authored module requests.
2. Core analyzes every admitted file once and exposes a matcher-independent
   module interface and function-effect summary.
3. Core links imports, exports, re-exports, identities, and supported call flow
   over typed resolution records.
4. Compiled matchers query the linked model. Findings remain owned by the file
   containing the primary event.

Files, module IDs, graph edges, findings, evidence, and diagnostics have stable
ordering. Ambiguous exports, unsupported module shapes, missing resolutions,
and exhausted budgets remain unknown; they never become guessed provenance.
Project diagnostics are separate from rule severity, and partial analysis is
reported as partial. Session workers release results through a deterministic
merge path with one shared outstanding-work bound, so worker count cannot
change the serialized result.

## Rules and host policy

Provider rules should use declarative core matchers. Add a generic matcher
primitive when the same semantic operation benefits multiple rules. Use a
provider callback only when the rule cannot be represented accurately by the
declarative API.

Host globals belong to provider catalog configuration, not individual
matchers. Core supplies conservative ECMAScript assumptions; providers add the
browser, Node.js, Electron, or Obsidian environment they require.

Rule factories use local IDs such as `network.request`. A `RuleCatalog`
validates and qualifies them as `js:network.request` or
`obsidian:network.request`. High-confidence rules form the recommended profile;
broader discovery rules require explicit heuristic opt-in. Confidence measures
the strength of identification, not the importance of the behavior.

## Precision and limits

Glass Lint is precision-first:

- strict matches require lexical identity, supported provenance, or connected
  flow at the use position;
- local lookalikes, shadowed globals, and invalidated aliases do not match;
- raw names, suffixes, and broad literal fragments are heuristic evidence;
- unknown, dynamic, ambiguous, unsupported, or exhausted analysis fails
  closed; and
- work, intermediate collections, evidence, and output are bounded and
  deterministic.

These are architectural invariants. New capabilities must not weaken existing
strict matchers or leak facts across bindings, assignments, scopes, files, or
control paths.

## Core Rust design

Core code should make domain ownership and invariants visible in its types:

- Put behavior on the struct or trait that owns the state. Keep a free
  function only when no single type is the natural owner.
- Introduce semantic newtypes when they distinguish domain concepts, validate
  construction, or hide repeated collection operations. Do not pass raw
  indexes, strings, tuples, or maps when their meaning or invariants matter.
- Encapsulate domain collections behind focused APIs. Callers should request a
  domain operation instead of repeating lookup, insertion, ordering, or budget
  logic.
- Keep modules cohesive and APIs narrow. Expose validated operations, not
  storage layout or analysis internals; default to private visibility.
- Keep functions at one abstraction level. Split large or deeply nested logic
  by named domain steps, while leaving genuinely cross-cutting operations at
  the coordinating layer.
- Use consistent domain vocabulary across types, methods, diagnostics, and
  tests. Avoid aliases that name the same concept differently.
- Centralize shared domain logic. Similar-looking implementations in multiple
  analysis paths are a signal to identify the common owner, not copy helpers.
- Represent expected failure explicitly and add context at crate boundaries.
  Unsupported semantics and budget exhaustion are domain outcomes, not reasons
  to panic.
- Remove obsolete paths after a clean migration. Compatibility wrappers and
  duplicate APIs require an explicit need.

These rules are not formatting preferences. Apply them where they clarify
ownership, enforce an invariant, reduce repeated logic, or make a public API
harder to misuse.

## Change policy

Breaking Rust APIs, schemas, rule IDs, and layouts are allowed during active
development. Make a clean break: update all callers, fixtures, adapters, tests,
and documentation together. Do not retain parallel paths by default.
