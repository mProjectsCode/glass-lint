# Codebase Readability Audit

Date: 2026-08-02

Scope: 416 Rust source files, approximately 82,433 lines, across the workspace. This is a read-only maintainability review; no Rust, test, configuration, dependency, or documentation source was changed. The previous audit's 14 findings were not carried forward because the corresponding refactors are present in history and the old entries were marked fixed.

## Summary

The workspace is generally well-factored at the crate level and has strong typed domain vocabulary in many of the recently refactored areas. The remaining readability cost is concentrated in internal analysis boundaries: positional tuples and raw maps still carry semantic state, a few aggregate constructors expose too much assembly detail, and orchestration code spans several independent lifecycle phases.

There are 12 open findings: 2 high, 7 medium, and 3 low. READ-001 through READ-005 are fixed. `cargo clippy --workspace --all-targets -- -D warnings` passes. The recommendations below are ordered by how much they affect future changes and how much semantic knowledge callers currently need to hold.

## Findings

### API and encapsulation

#### [x] READ-001 — Public matcher error exposes an inaccessible compiler error

- Severity: Medium
- Fix Complexity: Medium
- Category: API design, encapsulation
- Location: `glass-lint-core/src/api/rule/error.rs:30-60`; `glass-lint-core/src/api/compiler/validate/error.rs:5-11`

`MatcherBuildError` is part of the public rule API, but its `QueryCompileError` variant contains `crate::api::compiler::validate::QueryCompileError`, which is `pub(crate)` in a private compiler module. The variant is accepted only because it carries a local `#[allow(private_interfaces)]`, so callers can observe the variant name without being able to name or construct its payload type.

Recommendation: expose a stable public diagnostic type at the rule boundary, or translate the compiler error into a public rule-level error payload before constructing `MatcherBuildError`. Remove the targeted allow once the boundary has a type that downstream callers can actually use.

Fix Applied: `MatcherBuildError::QueryCompileError` now carries the public `QueryDiagnostic` type, and the compiler translates its private validation errors at the rule boundary. The targeted private-interface suppression and compiler-type leak were removed while preserving stable diagnostic codes and messages.

#### [x] READ-002 — Package-specifier grammar is implemented twice

- Severity: Medium
- Fix Complexity: Medium
- Category: Duplication, domain modeling
- Location: `glass-lint-core/src/api/rule/module.rs:34-81`; `glass-lint-core/src/project/types/input.rs:50-90`

`ModuleSpecifierPattern::package` and `PackageSpecifier::new` independently trim and validate overlapping package-name rules, including scoped names, slashes, relative-looking paths, and malformed inputs. They already differ in details such as whitespace and NUL handling, which makes future validation changes likely to drift even though both are expressing package identity grammar.

Recommendation: centralize the shared package grammar in one private semantic parser/newtype and map its validation failure into each owning API's error type. Keep exact module-pattern behavior separate where it intentionally accepts a broader authored module string.

Fix Applied: Added one crate-private `PackageName` parser for trimming, whitespace/NUL rejection, path-shape checks, and scoped-package grammar. `PackageSpecifier` and `ModuleSpecifierPattern::package` now delegate to it and translate failure into their owning error types, while exact module patterns remain independent.

#### [x] READ-003 — Projection planning uses positional tuples for distinct identities

- Severity: Medium
- Fix Complexity: Medium
- Category: Semantic newtypes, API clarity
- Location: `glass-lint-core/src/analysis/project/projection.rs:33-43,67-118`

`ProjectionPlan` stores constrained roots as `(usize, &PhysicalRoot)` and lifecycle roots as `(RuleIndex, usize, &CompiledObjectFlow)`. The integers represent different concepts—rule identity, physical-root position, and root selection—but the tuple shape does not preserve those distinctions, and construction repeatedly relies on positional destructuring.

Recommendation: introduce private plan records such as `PlannedConstrainedRoot` and `PlannedFlow` with named fields and typed indices. Let the plan own construction and accessors so matcher selection code cannot accidentally swap or reinterpret one index as another.

Fix Applied: `ProjectionPlan` now stores named constrained-root and lifecycle-flow records, with `RuleIndex`, `PhysicalRootIndex`, and flow references in explicit fields. Conversion to the existing matcher/projector input tuples is isolated in owner methods at the execution boundary, so plan construction no longer depends on positional identity semantics.

#### [x] READ-004 — Flow fixed-point snapshots encode domain state as nested tuples

- Severity: Medium
- Fix Complexity: Medium
- Category: Semantic newtypes, maintainability
- Location: `glass-lint-core/src/analysis/flow/projector/state.rs:35-47,261-302`

`CanonicalRequirements` is a vector of `(usize, Vec<FactId>)`, while `CanonicalFlowState` is a five-element tuple containing an object id, flow id, source event, requirements, and sinks. The snapshot is central to fixed-point convergence, so readers must remember tuple positions and the distinction between requirement and sink collections while reviewing correctness-sensitive code.

Recommendation: model the snapshot with named `CanonicalFlowState` and requirement-state types, preferably with constructors or normalization methods on `FlowStateTable`. Preserve the existing deterministic ordering and normalization behavior while making the convergence identity explicit in fields and methods.

Fix Applied: Replaced the canonical snapshot tuple aliases with named canonical object, alias, requirement, sink, and flow-state records. The fixed-point identity still normalizes projection-local object IDs and retains deterministic ordering, but convergence comparisons now expose their domain fields directly.

#### [x] READ-005 — Cross-file flow worklists and caches use anonymous composite keys

- Severity: Medium
- Fix Complexity: Medium
- Category: Semantic newtypes, ownership
- Location: `glass-lint-core/src/analysis/flow/cross/graph.rs:11-47`; `glass-lint-core/src/analysis/flow/cross/mod.rs:105-149`; `glass-lint-core/src/analysis/flow/cross/sources.rs:232-269`

The qualified call graph maps `(ModuleId, FactId)` to `(ModuleId, FunctionId)`, the cross-worklist cache is keyed by `(FlowId, ModuleId)`, and source propagation queues `(SourceKey, SourceCandidate)` pairs. These structures are core relationships in the cross-file fixed point, but their map and queue APIs expose meaning only through comments and tuple position.

Recommendation: add named records such as `QualifiedCallSite`, `QualifiedCallTarget`, `FlowPlanKey`, and `PropagationItem`, then centralize insertion and lookup on the owning types. This also gives the worklist a place to document ordering, deduplication, and budget semantics without repeating them at every call site.

Fix Applied: Added named qualified call-site/target records, a `FlowPlanKey` for the per-flow/module cache, and a `PropagationItem` for source worklist entries. These owners preserve the former sorted/hash key behavior while making call identity, cache scope, and propagation payload explicit.

#### [ ] READ-006 — OccurrenceIndex leaks its backing map to implement package scanning

- Severity: Medium
- Fix Complexity: Medium
- Category: Encapsulation, collection API
- Location: `glass-lint-core/src/analysis/matching/occurrence.rs:335-358`; `glass-lint-core/src/analysis/matching/query/view.rs:44-51`

`OccurrenceIndex<K>` wraps a `BTreeMap<K, Vec<Occurrence>>`, but exposes `as_map()` so the query view can pass storage directly into `BorrowedPackageOccurrenceIter`. That makes the package predicate, masking behavior, and base/overlay merge mechanics a consumer concern instead of an operation owned by the occurrence index.

Recommendation: replace `as_map()` with an owner-facing package-candidate iterator or a method that accepts the package predicate and overlay view. Keep the lazy merge and deterministic ordering internal to the occurrence abstraction, and expose only occurrence-oriented results.

Fix Applied: None so far.

#### [ ] READ-007 — Module identity merging reconstructs and merges raw entry tuples outside its owner

- Severity: Medium
- Fix Complexity: Medium
- Category: Encapsulation, duplicated domain logic
- Location: `glass-lint-core/src/analysis/matching/identity_map.rs:7-49`; `glass-lint-core/src/analysis/project/identities.rs:169-212`

`ModuleIdentityMap::into_entries()` turns nested map storage into `(ModuleExportKey, ExportResolution)` tuples, and `collect_exported_identities` consumes child maps, clones entries, detects star-vs-star conflicts, and then performs a second manual merge while preserving direct exports. The precedence and ambiguity rules therefore live in the caller even though the map owns the identity entries.

Recommendation: add a conflict-aware `merge_from` or visitor operation to `ModuleIdentityMap` that encodes direct-export precedence and ambiguity handling. Remove the storage-oriented `into_entries()` path once callers can express the merge in domain terms.

Fix Applied: None so far.

#### [ ] READ-008 — Flow property-write transitions are split between projector and state table

- Severity: High
- Fix Complexity: High
- Category: Ownership, state transitions
- Location: `glass-lint-core/src/analysis/flow/projector/mod.rs:710-756`; `glass-lint-core/src/analysis/flow/projector/state.rs:164-214`

`record_property_write` asks `FlowStateTable` for raw `FlowStateKey` values, looks up each flow plan itself, loops through compiled requirements, and separately calls `clear_requirement`, `record_requirement`, and `emit_if_ready`. The table owns the state and inverse log, while the projector owns a lifecycle transition that knows the table's key and mutation protocol in detail.

Recommendation: introduce a typed property-write transition and let the state owner apply matching requirements, returning the state changes or ready emissions needed by the projector. Keep plan-specific matching injected through a narrow operation rather than exposing raw state keys and three independent mutation methods to the orchestration layer.

Fix Applied: None so far.

#### [ ] READ-009 — ScopeGraphParts is a broad raw assembly aggregate

- Severity: High
- Fix Complexity: High
- Category: Architecture, encapsulation
- Location: `glass-lint-core/src/analysis/scope/graph.rs:77-93,289-303`; `glass-lint-core/src/analysis/scope/build/freeze.rs:53-114`

`ScopeGraphParts` exposes thirteen independent fields, including all major maps and indexes, and `ScopeGraph::from_parts` passes six of them positionally into `BindingIndex::new`. The freeze phase must know the complete storage layout and the meaning of a raw `(FunctionId, NameId)` parameter-alias key, so adding or reordering one field has a wide, low-signal change surface.

Recommendation: replace the broad parts bag with private semantic sub-aggregates such as scope indexes, binding data, and mutation data, or provide an owning builder that performs the transitions. Encode parameter aliases in a named key type and keep raw map construction inside the type that owns the invariant.

Fix Applied: None so far.

### Scope, trace, and result modeling

#### [ ] READ-010 — Scope collection hands off anonymous lifecycle records

- Severity: Medium
- Fix Complexity: Medium
- Category: Semantic newtypes, boundary design
- Location: `glass-lint-core/src/analysis/scope/build/mod.rs:47-54,114-125`; `glass-lint-core/src/analysis/scope/build/assignments.rs:463-484`; `glass-lint-core/src/analysis/scope/graph.rs:160-225`

The collector stores function checkpoints as `(CollectorCheckpoint, u32, usize)` and dynamic evaluations as `(ScopeId, ScopeEffect)`, then later destructures, sorts, filters, and groups those records in separate modules. The tuple positions encode conditional depth, control-flow depth, scope ownership, and effect identity without a type expressing those invariants.

Recommendation: add `FunctionCheckpoint` and `ScopedDynamicEval` records with named fields and methods for restore/grouping. Let collection and freezing consume those records through semantic operations rather than repeating tuple sorting and destructuring logic.

Fix Applied: None so far.

#### [ ] READ-011 — Trace reconstruction exposes a public tuple protocol

- Severity: Low
- Fix Complexity: Low
- Category: API clarity
- Location: `glass-lint-core/src/analysis/trace.rs:70-83`; consumer at `glass-lint-core/src/lint/report.rs:359-383`

`TraceArena::reconstruct_trace` returns `Vec<(QualifiedEvent, EvidenceRole)>`, and report assembly immediately destructures the pair to recover location and message semantics. A trace step is a stable domain concept, so a public tuple makes the relationship between event and role dependent on position and leaves no owner for future trace metadata.

Recommendation: return a private or crate-visible `TraceStep` with named `event` and `role` fields, or expose an iterator consumed by a trace-to-evidence method. Keep report rendering out of the raw arena representation.

Fix Applied: None so far.

#### [ ] READ-012 — ProjectionOutcome mixes status, counters, and report metrics in one mutable DTO

- Severity: Medium
- Fix Complexity: Medium
- Category: API design, invariants
- Location: `glass-lint-core/src/analysis/project/projection.rs:202-231,233-275,394-418`; `glass-lint-core/src/lint/report.rs:500-513`

`ProjectionOutcome` combines exhaustion flags and observed budgets with effect/projection counts, trace counts, fixed-point counters, and private module lists. Report assembly reads several fields directly while project status recording reads a different subset, so the struct is both a status result and a metrics transport with no strong separation of invariants.

Recommendation: split status into a `ProjectionStatus` value and counters into a `ProjectionMetrics` value, with merge and recording methods on each. Make fields private and expose focused getters or report/status conversion methods so callers cannot construct a contradictory outcome by editing independent flags and counts.

Fix Applied: None so far.

#### [ ] READ-013 — Report assembly combines linking, matching, traces, diagnostics, and metrics

- Severity: Medium
- Fix Complexity: Medium
- Category: Module cohesion, architecture
- Location: `glass-lint-core/src/lint/report.rs:99-161,163-397,399-468,471-528`

`ReportAssembly::finish` orchestrates project linking and matching, while the same module also groups evidence, resolves and falls back traces, initializes files, attaches diagnostics, and computes operation metrics. These phases have different inputs, failure behavior, and likely change drivers, making the 528-line module a high-traffic seam for unrelated modifications.

Recommendation: split private modules for evidence-to-finding conversion, trace rendering, diagnostic attachment, and final project-report assembly. Keep `ReportAssembly::finish` as a small orchestration facade whose sequence and completion semantics remain easy to inspect.

Fix Applied: None so far.

### Harness and test boundaries

#### [ ] READ-014 — Harness case loading mixes snippet directives with project protocol loading

- Severity: Medium
- Fix Complexity: Medium
- Category: Module cohesion, test infrastructure
- Location: `glass-lint-harness/src/cases.rs:35-569`

The file parses single-file source directives and expectations, then defines and deserializes project manifests, builds resolution protocol records, loads project files, and merges tool expectations. Snippet parsing and project-fixture loading have different schemas and error paths, but share one large module and several generic helpers.

Recommendation: move snippet parsing and project-manifest loading into private `snippet` and `project` modules, retaining `cases.rs` as the stable loading facade. Keep shared language/path helpers in a small common module so the split does not duplicate fixture conventions.

Fix Applied: None so far.

#### [ ] READ-015 — Profile runner combines workload dispatch, preparation, execution, and aggregation

- Severity: Medium
- Fix Complexity: Medium
- Category: Module cohesion, concurrency readability
- Location: `glass-lint-harness/src/profile/runner.rs:39-630`

The runner handles file discovery and manifest verification, loader-project and admitted-project modes, linter construction, warm-up/repetition timing, worker scheduling, report accumulation, and deterministic result sorting. The concurrency code is bounded and understandable locally, but it is embedded in a module that also owns workload policy and output aggregation.

Recommendation: split preparation, workload-specific runners, worker execution, and summary aggregation into private modules with narrow result types. Keep timing and ordering rules in the type that owns each phase, and leave the public entry point responsible only for dispatch and error propagation.

Fix Applied: None so far.

#### [ ] READ-016 — Large integration test modules mix multiple semantic contracts

- Severity: Low
- Fix Complexity: Medium
- Category: Test organization
- Location: `glass-lint-core/tests/integration/matching/declarative.rs:1-1162`; `glass-lint-project/src/tsconfig/tests.rs:1-1039`; `glass-lint-core/src/api/compiler/tests/normalize.rs:1-958`; `glass-lint-core/src/api/compiler/tests/validate.rs:1-935`

These test modules cover several distinct contracts in one source file: public matcher construction, scope/provenance, argument values, lifecycle flow, module identity, tsconfig parsing/extends/merging, and multiple compiler phases. The shared setup is useful, but failures and future additions require scanning very large files to find the relevant semantic boundary.

Recommendation: split tests by contract into submodules or files such as matcher arguments, lifecycle, module identities, tsconfig inheritance, normalization, and validation. Preserve the existing public/API-level coverage and put only genuinely shared builders in support modules.

Fix Applied: None so far.

### Maintenance hygiene

#### [ ] READ-017 — Lint suppressions remain after signature and decomposition refactors

- Severity: Low
- Fix Complexity: Low
- Category: Dead/obsolete code
- Location: `glass-lint-core/src/analysis/matching/arguments/mod.rs:88-95`; `glass-lint-core/src/analysis/project/projection.rs:279-290`; `glass-lint-core/src/analysis/scope/binding_index.rs:24-32`

`compute_constrained_inner` currently accepts three arguments but still carries `#[allow(clippy::too_many_arguments)]`; the test-only `ProjectSemanticModel::project` wrapper is only a few lines but retains `#[allow(clippy::too_many_lines)]`. `BindingIndex::new` also retains a broad argument-count suppression after the surrounding aggregate refactor, so these allowances no longer clearly document an unavoidable exception.

Recommendation: remove each suppression and rerun the workspace lint gate, retaining only allows that still correspond to a measured warning. Where an allowance remains necessary, add a short reason tied to the invariant or unavoidable API shape so later refactors can distinguish intentional exceptions from residue.

Fix Applied: None so far.

## Systemic Themes

- Semantic state is often typed at the leaf level but loses its vocabulary at orchestration boundaries. The next useful refactors are named records and owner-facing operations, not more wrapper methods around raw `HashMap`/`BTreeMap` storage.
- Several recent decompositions improved individual functions but left broad construction aggregates or direct field reads at the next boundary. Encapsulation should follow the state owner through freeze, projection, and reporting, not stop at the first extracted helper.
- Analysis behavior is deliberately bounded and deterministic, and the code generally makes those constraints visible. New abstractions should preserve sorted iteration, explicit budgets, and the existing distinction between definite and possible evidence.
- No `Rc` or `RefCell` usage was found in the Rust workspace. The observed `Arc`, `Mutex`, `OnceLock`, and atomic/barrier usage is concentrated in shared caches and profiling workers and does not currently present as a readability hotspot.

## Resolved Decisions

- **Public diagnostics:** keep `QueryCompileError` private and convert it at the compiler/rule boundary into the existing public `QueryDiagnostic` type. `MatcherBuildError` should expose the stable diagnostic rather than an inaccessible compiler implementation type.
- **Package grammar:** keep `PackageSpecifier` and `ModuleSpecifierPattern` as separate semantic types, but centralize their shared package-root validation in one internal parser/newtype. Preserve `ModuleSpecifierPattern::exact` as the broader exact-module-path API.
- **Internal aggregates:** do not retain broad raw construction seams. Move `ScopeGraphParts` behind an owner-controlled builder or semantic sub-aggregates, and split `ProjectionOutcome` into private status and metrics components with focused accessors.
- **Implementation order:** address the public diagnostic boundary first, package validation second, and scope/projection aggregate encapsulation third; add focused tests before running the full gate.

## Coverage

Reviewed repository guidance (`ARCHITECTURE.md`, owning crate architecture documents, `TESTING.md`, and `CONTRIBUTING.md`), the existing audit, recent refactor history, all Rust file paths, largest source modules, public and crate-visible analysis boundaries, tuple/raw-map APIs, lint suppressions, harness/profile code, integration tests, and workspace clippy output. The working tree was clean before this implementation pass; READ-001 through READ-005 changed core boundaries and this report.

Verification for the audit baseline: `make ci` passed, including workspace tests, doctests, e2e/provider harness verification, rule generation checking, and example compilation. For READ-001, compiler/public-surface tests passed; for READ-002, package unit and integration tests passed; for READ-003, the core test target compiled; for READ-004, all projector state and flow tests passed; for READ-005, all cross-flow tests passed. Core clippy passed with warnings denied; `cargo fmt --all` and `git diff --check` passed.
