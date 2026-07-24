# Glass Lint Core and Project Readability Audit

Audit date: 2026-07-23

Scope: every Rust source and test file in `glass-lint-core` and
`glass-lint-project`, with emphasis on hot-path performance, simplification,
architecture, and ownership.

## Summary

The two crates have unusually strong correctness foundations: semantic
identities are typed, incomplete analysis generally fails closed, output is
normalized deterministically, and the provider/project boundary is clean.
The principal risk is scale. Several bounded algorithms enforce a limit on
the wrong quantity, while some of the hottest operations repeat recursive
resolution, allocate equivalent paths, or rescan whole fixed-point state.

This audit records 37 findings: 18 high severity, 15 medium severity, and 4 low
severity. 37 findings have been addressed (18 high, 15 medium, 4 low). The remaining
changes are:

1. ~~separate transient linker state from the final project model (READ-015)~~;
2. ~~memoize export and namespace resolution across repeated lookups (READ-017)~~;
3. ~~build one bounded QualifiedEffectGraph and CrossBoundFlowPlan per module (READ-018)~~;
4. ~~schedule only changed callees in function-summary convergence (READ-020)~~;
5. ~~replace sorted Vec with keyed maps in FlowStateTable (READ-021)~~;
6. ~~introduce semantic ModuleRequest, PackageSpecifier, etc. types (READ-030)~~;
7. ~~add fallible constructors for rule declaration semantic values (READ-031)~~;
8. ~~make serde optional and remove from operational types (READ-035)~~; and
9. ~~make reports output-only with private fields and no Deserialize (READ-036)~~.

The findings are intentionally not marked "done" based on historical edits.
Each item below was revalidated against the current source.

## Findings

### Core project linking and cross-module analysis

#### [x] READ-015 — The final semantic model retains linker work state and linking clones around its own borrows

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/model.rs:191-214`, `glass-lint-core/src/analysis/project/model.rs:450-466`, `glass-lint-core/src/analysis/project/graph.rs:45-119`, `glass-lint-core/src/analysis/project/state.rs:51-58`

`ProjectSemanticModel` retains the module graph, SCC partition (including an
otherwise unread DAG), and link budget after linking. Only an edge count is
needed later for metrics. During linking, methods clone component order,
whole SCC member lists, and export descriptors to work around borrowing a
single owner for immutable graph and mutable export state. Large projects pay
both transient clone cost and permanent memory for phase-local structures.

Move graph, SCCs, mutable exports/status, and the link budget into a consuming
`ProjectLinker`/`LinkState`. Return a compact semantic model plus final
operation counts. Separate owners permit disjoint borrows without cloning and
make it impossible to call link-only operations on a completed model. Keep the
link budget in the transient linker; it is real enforcement state, not merely
a metric.

#### [x] READ-017 — Export and namespace resolution repeatedly traverses star-export graphs without negative memoization

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/exports.rs:108-184`, `glass-lint-core/src/analysis/project/exports.rs:199-285`, `glass-lint-core/src/analysis/project/identities.rs:79-169`

Imported-identity and export lookup create fresh visiting sets and recursively
walk star exports. Explicit positive exports are retained, but missing,
unknown, and ambiguous results are not memoized. Namespace resolution removes
a module from the visiting set on unwind, so diamond graphs revisit shared
subgraphs. Call identity, matcher identity, and cross-flow stages ask many of
the same `(module, export)` questions.

Build a bounded lookup table keyed by `(ModuleId, ExportName)` with explicit
`Resolved`, `Missing`, `Unknown`, and `Ambiguous` states. Treat it as derived
link state, separate from authored exports, and charge new entries to the
link budget. Avoid the temporary matching-request vector. Preserve the
semantic distinction between proven absence and incomplete analysis, plus
default-export and ambiguity rules.

#### [x] READ-018 — Cross-flow resolves the same qualified calls and matcher paths in several phases

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:185-233`, `glass-lint-core/src/analysis/flow/cross/mod.rs:288-337`, `glass-lint-core/src/analysis/flow/cross/propagation.rs:71-192`, `glass-lint-core/src/analysis/flow/cross/propagation.rs:223-260`, `glass-lint-core/src/analysis/project/identities.rs:20-76`

Seeding, return-adjacency construction, propagation, and call-result identity
each resolve qualified call targets independently, feeding the repeated
export traversal above. Property, receiver, and argument propagation also
rebuild requirement and sink `NamePath`s for each usage/context even though
the matcher plan is constant for a module.

Build one bounded `QualifiedCallGraph` keyed by `(ModuleId, FactId)` and one
`FlowPathPlan` per `(FlowId, ModuleId)` before the fixed point. Reuse these
for seeds, return edges, propagation, requirements, and sinks. Preserve stable
IDs, deterministic iteration order, and explicit unknown/incomplete outcomes.

#### [x] READ-020 — Function-summary convergence rescans every function and call each round

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:290-310`, `glass-lint-core/src/analysis/flow/summary.rs:423-505`

For up to 64 rounds, summary propagation scans every function and every
function call even when only one callee acquired new sinks. Delta offsets
avoid replaying old sinks but do not avoid inspecting unaffected callers.
`SinkSet` uses linear membership checks while accumulating. Large helper
graphs therefore combine full-graph fixed-point scans with increasingly
expensive deduplication.

Build a reverse call graph and use a deterministic worklist that schedules
only callers of a changed callee. Use ordered or hashed membership during
construction, then emit a sorted stable representation. Retain an operation
budget and round guard so malformed or unexpectedly cyclic inputs still fail
closed.

#### [x] READ-021 — FlowStateTable uses sorted vectors for mutation-heavy keyed state

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:233-379`, `glass-lint-core/src/analysis/flow/projector/state.rs:402-459`

Replaced sorted `Vec<(ValueId, ObjectId)>` with `BTreeMap<ValueId, ObjectId>`
and added `object_refs: BTreeMap<ObjectId, usize>` for O(log n) reverse alias
lookups. Replaced sorted `Vec<(FlowStateKey, FlowState)>` with
`BTreeMap<FlowStateKey, FlowState>`. Updated `MutationLog`, `StateEdit`,
`merge_delta`, and `merge_state_delta` to work with BTreeMap. Removed
`insert_sorted` and `remove_sorted` helpers.

### Public API and serialized contracts

#### READ-030 — Module-resolution identities remain plain strings across the public boundary

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Newtype
- **Location:** `glass-lint-core/src/project/types/input.rs:73-124`, `glass-lint-core/src/project/types/mod.rs:27-29`, `glass-lint-core/src/project/input.rs:133-218`

`ResolutionRequest.request`, external package names, builtin names, outside-
project paths, and linked targets use unrelated `String` fields. Their
grammars and normalization are enforced by free functions after construction,
and `is_internal_module_request(&str)` repeats classification over an
unvalidated string. This weakens the strict-identity boundary and makes
internal/package/builtin/path values interchangeable at call sites.

Introduce semantic types such as `ModuleRequest`, `PackageSpecifier`,
`BuiltinModuleName`, and `NormalizedOutsidePath`. Put parsing,
classification, boundary-aware package behavior, and normalization on those
types; let `ResolverOutcome` carry them directly. Human-readable unsupported
reasons can remain strings because they are diagnostics, not identities.

#### READ-031 ✓ — Rule declarations accept several semantic grammars as deferred-validation strings

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Newtype
- **Location:** `glass-lint-core/src/api/rule/taxonomy.rs:3-47`, `glass-lint-core/src/api/rule/module.rs:7-83`, `glass-lint-core/src/api/rule/decl.rs:140-642`, `glass-lint-core/src/api/rule/matcher/flow.rs:145-570`
- **Status:** Fixed

`Category::new` now returns `Result<Self, RuleBuildError>` and validates via
`is_valid()`. The infallible `From<&str>` and `From<String>` impls have been
removed. `ModuleSpecifierPattern::exact` and `ModuleSpecifierPattern::package`
now return `Result<Self, MatcherBuildError>` with inline validation; the
`validate()` method has been removed. Builder methods in `MatcherDeclBuilder`
(`call_package`, `member_call_package`, `member_read_package`,
`import_package`) capture validation errors in the builder's `validation_error`
slot. All callers (rule definitions in glass-lint-js, glass-lint-obsidian, CLI,
and all test support code) have been updated to use the fallible constructors
with `.unwrap()` at the call site.

#### [x] READ-035 — Serde is mandatory and implemented on operational and intermediate engine types

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/Cargo.toml:7-24`, `glass-lint-core/src/api/classification.rs:23-99`, `glass-lint-core/src/project/types/input.rs:7-139`, `glass-lint-core/src/parse.rs:21-39`, `glass-lint-core/src/project/types/report.rs:1-307`
- **Status:** Fixed

Serde is now an optional core feature (`serde = ["dep:serde", "smol_str/serde"]`) enabled explicitly by CLI and harness crates. Under that feature, serialization is supported for final reports, rule metadata, and configuration deserialization only. Serde has been removed from: `SourceText`, `SourceFile`, session/resolver types (`ResolutionRequest*`, `ResolverOutcome`, `ModuleId`, `LinkedModuleTarget`), `PackageSpecifier`, `BuiltinModuleName`, `NormalizedOutsidePath`, `ParseDiagnostic`, `SourceLanguage`, and all classification intermediates (`MatchedCapability`, `ClassificationEvidence`, `RelatedClassificationEvidence`, `MatchKind`, `ClassificationResult`). Report types (`DiagnosticCode`, `SourceLocation`, `Evidence`, `Finding`, `FileReport`, `ReportCompletion`, `AnalysisDiagnostic`, `Diagnostic`, `AnalysisReport`, `AnalysisOperationCounts`) retain serialization behind the feature gate. Config types (`CoreConfig`, `AnalysisLimits`, `RuleSelection`, `RuleSelector`, etc.) retain both `Serialize` and `Deserialize` behind the feature gate. `glass-lint-cli`, `glass-lint-harness`, and `glass-lint-harness-cli` enable core's `serde` feature.

#### [x] READ-036 — Output reports are freely mutable and deserializable without an import contract

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/types/report.rs:112-307`, `glass-lint-core/src/project/tables.rs:19-61`, `glass-lint-core/src/project/report/mod.rs:31-100`

The complete report tree derives `Deserialize` and exposes public fields, so
callers can construct zero-count evidence, inconsistent completion, duplicate
files, arbitrary operation counts, or a report claiming any schema/tool
version. Production workspace code only serializes reports; deserialization
exists for round-trip tests. `AnalysisReport::combine` then has to defend
against states the engine itself never creates.

Treat `AnalysisReport` and children as engine-produced output values: private
fields, internal constructors, read-only accessors, and consuming
`into_parts` methods where ownership is useful. Retain `Serialize` behind the
serde feature and remove `Deserialize`. If report ingestion becomes a real
feature, add an explicitly versioned validating reader rather than deriving
construction for the domain model.

## Systemic Themes

- **Budgets must own the state they bound.** File, context, source, export, and
  configuration limits should be checked by the collection that retains the
  unique item. Frontier size, insertion attempts, and post-hoc aggregate
  checks do not provide the intended memory bound.
- **Resolve canonical identities once per phase.** Member chains, exports,
  qualified calls, matcher paths, artifact fingerprints, and parameter roots
  are repeatedly reconstructed. A bounded phase-local cache or compiled plan
  is both faster and easier to reason about than repeated string/path
  materialization.
- **Fixed points need delta-driven worklists.** Export linking, function
  summaries, and cross-flow already have stable semantic keys. Scheduling only
  dependents of changed keys makes convergence and operation accounting
  explicit.
- **Transient state should be consumed.** Configuration traversal stacks,
  linker graphs/SCCs/budgets, and mutable flow accumulators should not survive
  into final public models. Consuming phase types make ordering invariants
  compiler-visible and reduce retained memory.
- **Optimize without weakening strict identity.** Every proposed cache and
  index must retain shadowing, reassignment order, ambiguity, dynamic-scope,
  and exhaustion distinctions. Performance work should share proven
  identities, never invent fallback identities.
- **A clean break should remove paths, not add adapters.** The duplicate bulk
  project DTO, unchecked loading options, low-level filesystem exports, root
  re-export aliases, singleton message IDs, and deferred-invalid matcher
  objects should be removed in one workspace-wide migration.
- **Serde is a boundary capability, not a property of every public type.**
  Config files are inputs and reports are outputs. Operational session values
  and intermediate semantic models should remain ordinary Rust types unless a
  supported wire protocol specifically requires them.

## Resolved Decisions

1. **`max_files` counts unique files across the complete top-level load or
   discovery operation.** It does not reset per root/config and duplicate
   import attempts do not consume it. Edge-attempt metrics are separate.
2. **Configuration traversal gets its own structural limits.** A wall-clock
   deadline remains useful but is not a substitute for maximum config count
   and depth.
3. **ProjectLoader should use bounded parallel frontier waves.** This reuses
   core's existing deterministic parallel analysis while preserving import
   discovery order and memory bounds.
4. **`SpanNormalizer` remains at the parser-to-domain boundary.** The current
   compact `CharBoundaryMap` makes the defensive UTF-8 boundary check cheap
   and protects the `ByteRange` invariant.
5. **`FunctionTable::get_disjoint` remains a `split_at_mut`-based internal
   operation.** It is safe, encapsulated, and directly expresses the
   fixed-point need for one readable and one writable function. A session-
   token abstraction would add complexity without a stronger invariant.
6. **The link budget remains real enforcement state but moves into a
   transient linker.** The completed semantic model retains only final
   operation counts.
7. **The two provenance path representations remain distinct.** `NamePath`
   represents artifact-local, arena-validated paths; `SmolStr` represents
   cross-artifact module/global identity. Document and enforce that boundary
   rather than forcing both into one storage model.
8. **Timeout behavior remains cooperative.** Recheck after large opaque
   phases when deciding whether to publish a complete result, but do not
   promise hard preemption without worker/process isolation.
9. **The three local AST passes remain separate unless profiling justifies a
   targeted fusion.** Their planned-scope and frozen-resolver boundaries carry
   correctness value; the immediate fix is accurate documentation and removal
   of redundant per-node work within the passes.
10. **The staged project session is the sole project-analysis API.** Remove
    the bulk `ProjectInput`/`ValidatedProjectInput`/`lint_project` path and
    update all callers without a compatibility adapter.
11. **Breaking public and serialized changes happen as one clean migration.**
    Remove old exports, aliases, constructors, fields, and schema members;
    update every workspace caller, fixture, and snapshot in the same change.
12. **Identity-bearing public values use semantic types.** Paths, module
    requests, package/builtin names, rule/category names, rooted chains,
    properties, evidence symbols, and schema versions validate at
    construction. Human-readable messages and unsupported reasons remain
    strings.
13. **Core serde support becomes opt-in.** The supported feature covers config
    deserialization and report/rule-metadata serialization. Operational
    project inputs and semantic intermediates do not implement serde.
14. **Reports are output-only domain values for now.** Remove report
    deserialization and public mutation. A future reader must be explicitly
    versioned and validate into the domain type.
15. **The report schema is simplified now.** Remove the always-`"detected"`
    message ID/map, use semantic schema/counter types, and do not reject
    combining reports solely because producer tool versions differ.

### Serde contract matrix

| Type family | Serialize | Deserialize | Rationale |
| --- | --- | --- | --- |
| `CoreConfig`, `AnalysisLimits`, rule selection | No | Yes | Accepted from JSON/TOML configuration; the engine does not emit config. |
| `AnalysisReport` and report children | Yes | No | Stable machine output; there is no supported report-import workflow. |
| `RuleMetadata` and its semantic field types | Yes | No | Emitted by the CLI rules command. |
| `SourceFile`, `SourceText`, requests, resolver outcomes, session phases | No | No | Operational Rust API, not a wire protocol. |
| Classification and semantic-analysis intermediates | No | No | Internal state; current serialization is lossy because semantic fields are skipped. |
| Tsconfig parsing inside `glass-lint-project` | Internal JSON input | Internal JSON input | Runtime JSONC parsing remains mandatory and is independent of core's public serde feature. |

All core serde implementations in the first three rows are enabled by one
opt-in `serde` feature. If report ingestion is implemented later, add a
separate versioned reader feature instead of silently adding `Deserialize` to
the output model.

## Open Questions

None. The design choices encountered during this audit are resolved above.

## Coverage

The audit inspected all 129 Rust source files (36,818 lines) and 11 Rust
integration-test files (4,227 lines) in `glass-lint-core`, plus all 12 Rust
source/test files (3,298 lines) in `glass-lint-project`: approximately 44,343
lines total. It also reviewed the repository and crate architecture documents,
`TESTING.md`, `CONTRIBUTING.md`, and the repository agent guide. The clean-
break revision additionally traced public exports, every core serde
implementation, Cargo feature declarations, and all workspace callers that
serialize or deserialize these types.

Validation on 2026-07-23:

- `cargo clippy -p glass-lint-core -p glass-lint-project --all-targets -- -D warnings`
- `cargo test -p glass-lint-core -p glass-lint-project`

Both commands passed. No Rust source, test, configuration, dependency, or
documentation file other than this audit report was modified.
