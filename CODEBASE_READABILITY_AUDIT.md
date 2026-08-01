# Codebase Readability Audit

## Summary

This report replaces the previous audit. The previous version described several completed refactors, but the current tree still contains a number of the underlying representation and ownership patterns, so its historical findings were not carried forward as evidence of the present state.

The highest-impact issues are the leaking path-storage representation, evidence storage that falls back to raw nested vectors, and phase coordinators that still own too many independent pieces of flow state. The project loader, profiling harness, and provider catalogs have similar but more localized aggregation and duplication patterns. These are maintainability findings rather than claims of functional defects; the recommended changes should preserve bounded analysis, path-local identity, fail-closed behavior, and deterministic output.

## Findings

### Core storage and API boundaries

#### [x] READ-001 — Path storage still exposes tagged IDs and trusted cross-store construction

- **Severity:** High
- **Fix Complexity:** High
- **Category:** API
- **Location:** `glass-lint-datastructures/src/path_trie/types.rs:5-31`; `glass-lint-datastructures/src/path_trie/store.rs:13-17,32-47,70-216`; `glass-lint-datastructures/src/path_trie/interner.rs:5-75`; `glass-lint-core/src/analysis/flow/summary/store.rs:1-243`

`PathId` and `SummaryPathId` still encode storage identity with raw integers and tag bits, while `PathInterner::store()` exposes the underlying `ParentPathStore`. `ParentPathStore::append_linked` accepts a caller-supplied parent depth, and the summary store duplicates frozen/overlay tag dispatch and calls that trusted operation directly; this makes cross-store identity and depth invariants depend on caller discipline. The public `as_u32`, `untag`, `parent`, and store-forwarding methods also make the representation part of the effective API.

Keep `ParentPathStore` reusable, but give it a safe public API: opaque path handles, validated parent ownership, derived depth, checked capacity, and operations that do not require callers to pass tags or trusted metadata. Remove `PathInterner::store()` and expose typed frozen/overlay/view operations instead; centralize tag dispatch in the owner so IDs from different stores cannot be mixed accidentally. Preserve a low-level reusable type only if its invariants are enforced at construction and every fallible operation reports exhaustion without panic; do not solve the boundary problem by making the reusable primitive private and duplicating it elsewhere.

Implemented store-owned opaque `PathId` handles and validated `PathLink` parent references with derived depth and checked capacity. `ParentPathStore` now records local versus linked parents explicitly, `PathInterner::store()` is gone, and `SummaryPathId` uses typed frozen/overlay variants instead of raw tag dispatch; focused tests cover cross-store rejection, frozen references, overlay joins, and exhaustion behavior.

#### [x] READ-002 — Rule evidence still falls back to raw nested vectors

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/classification.rs:72-113`; `glass-lint-core/src/analysis/project/projection.rs:149-157`; `glass-lint-core/src/analysis/matching/arguments/mod.rs:36-164`; `glass-lint-core/src/analysis/flow/projector/mod.rs:222-232`; `glass-lint-core/src/analysis/flow/projector/state.rs:347-395`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:94-166`

`RuleEvidenceTable` provides a typed owner, but `as_mut_slices`, `Index<usize>`, and several downstream APIs immediately expose or accept `&mut [Vec<ClassificationEvidence>]`. The local projector, argument matcher, and cross-file evidence collector then index those vectors with raw `usize`, so rule capacity, ordering, and vector alignment remain hidden invariants spread across multiple phases.

Carry `RuleIndex` and an evidence owner through these boundaries with methods such as `for_rule_mut`, `record`, `merge`, and `into_report`. If a raw slice is unavoidable for a narrow compiler or serialization bridge, keep that adapter private, make it one-way, and have all analysis code use typed rule operations; add a single invariant check at the owner boundary rather than repeating capacity assumptions in each phase.

Implemented typed `RuleEvidenceTable` operations (`for_rule_mut`, `record`, and `replace`) and removed the production mutable-slice/index boundary. Constrained matching, local flow, and cross-file evidence now pass rule-indexed owners; flow evidence keys carry `RuleIndex`, while the legacy `usize` index adapter remains test-only for assertions and no analysis phase indexes the nested vectors directly.

### Core analysis and flow coordination

#### READ-003 — `ObjectFlowProjector` remains a broad phase coordinator

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:129-220,298-509,511-694`

`ObjectFlowProjector` still owns the fact stream, names, plan, call index, evidence, flow-state table, lifecycle state, control frames, paths, pending calls, active-path counters, binding slots, trace arena, and module identity. Methods such as `finish_loop`, `transfer_function`, `join_paths`, `finalize_pending`, and `record_property_write` combine scheduling, fixed-point replay, state restoration, cleanup, evidence emission, and reporting, so the type is the implicit owner of several independent state machines.

Split the state into explicit owners for the path frontier, loop fixed-point/replay engine, pending evidence, and lifecycle outcome, leaving the projector as a thin coordinator over typed operations. Preserve the single fact stream and existing bounded behavior, but make restoration, join, and cleanup belong to one subtype each; return typed termination results instead of communicating phase outcomes through shared fields and counters.

#### [x] READ-004 — Declaration classification duplicates precedence across expression shapes

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/analysis/classification.rs:45-265`

`klassify_call`, `klassify_member`, and `klassify_ident` each probe overlapping provenance, `require`, returned-object, constant, rooted-name, and value-alias sources. Their checks and fallback order differ slightly, so adding or reordering one semantic source requires coordinated edits in three helpers; the owned `name` handling and repeated clones make that drift less visible.

Normalize the declaration into a small classification context containing the expression shape and borrowed name, then run one ordered candidate classifier. Keep shape-specific extraction helpers only for facts that genuinely require call or member syntax, and preserve the current precedence with focused tests for aliases, shadowing, reassignment, dynamic values, and unsupported shapes.

Implemented by borrowing the declarator name once and routing all declaration shapes through one ordered candidate classifier for module aliases, requires, callable bindings, constants, returned objects, static object values, and rooted aliases. Call/member-specific code now only selects precedence and handles the bind-call boundary; the existing scope-analysis tests continue to cover aliases, reassignment, destructuring, returned objects, and dynamic values.

#### [x] READ-005 — Summary sink propagation couples fixed-point policy and storage

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary/summaries.rs:19-72,142-269`

`FunctionSummaries` owns path storage, function summaries, scratch projections, exhaustion state, and total sinks, while `propagate_sinks` also constructs the reverse-call worklist, manages rounds, applies the sink cap, and projects calls. The same propagation routine therefore controls summary representation, fixed-point scheduling, resource exhaustion, and the semantics of `MAX_SUMMARY_SINKS`, making changes to one policy likely to affect the others.

Introduce a `SummaryPropagation` context that owns the worklist and round budget and returns a typed stop or completion result to a separate summary store. Keep sink limits, path limits, and call-round limits as distinct named policies, and make scratch projection lifetime and merge behavior explicit rather than fields on the long-lived summary owner.

Implemented with a `SummaryPropagation` scheduler that owns reverse-call edges, the deterministic worklist, and typed completion/exhaustion outcomes. Sink projection scratch state is now call-local, and the propagation worklist has its own named bound distinct from `MAX_SUMMARY_SINKS`; the existing bounded sink, path, and budget behavior remains covered by the summary propagation tests.

#### READ-006 — Cross-file flow passes overlapping argument bags between phases

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:52-98`; `glass-lint-core/src/analysis/flow/cross/propagation.rs:32-45,47-237,239-291`

`ContextProjection`, `UsageProjector`, and `CallPropagation` each carry large groups of borrowed state, with overlapping project, evidence, graph, arena, names, worklist, and flow fields. `ContextProjection::project` assembles these bags and several methods rebuild similar emission context values, so phase ownership and mutation order are expressed by construction-site wiring rather than by the API.

Let a `CrossProjectionSession` own the shared graph, evidence, arena, names, and worklist, and pass a smaller per-context state to usage and call propagation. Separate state transitions from emission through methods on those owners, and make the session’s lifecycle and worklist stop result explicit so adding a new propagation phase does not require expanding every constructor.

### Project loading and operational tooling

#### [x] READ-007 — Tsconfig graph traversal mixes graph policy with project admission

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/discovery.rs:162-313`

`collect_tsconfig_graph` simultaneously manages the traversal stack, depth and project budgets, cycle detection, visited configuration state, parsing and merging, source selection, reference scheduling, canonicalization, and diagnostics. It also mutates `ProjectDiscovery` admission state while constructing `TsconfigTraversal` inline, so filesystem policy, graph traversal, and project-budget policy are difficult to exercise or evolve independently.

Extract a `TsconfigGraphWalker` that owns visited/active/stack state and returns a typed per-config expansion containing source selection, references, and diagnostics. Leave `ProjectDiscovery` responsible for admission, deadlines, and project limits; retain the current DFS order, cycle diagnostics, and fail-closed budget behavior at the boundary between the two types.

Implemented `TsconfigGraphWalker` as an iterative, budget-aware owner of visited and active configuration state, traversal scheduling, canonical reference handling, and diagnostics. It now returns ordered typed expansions to `ProjectDiscovery`, which alone performs source admission; cycle diagnostics remain deterministically sorted and deduplicated, and the existing project tests continue to verify fail-closed budgets.

#### [x] READ-008 — Profiling uses several bespoke aggregate paths and a tuple result

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-harness/src/profile/runner.rs:188-345,635-668`; `glass-lint-harness/src/profile/types.rs:15-86,223-245`

`profile_projects`, `profile_loader_project`, and `aggregate_workload_results` each initialize and merge overlapping totals, phase timings, operation counts, completion state, and report data. `profile_loader_project` returns a five-element tuple and repeats report accumulation for repetition and overall results, while `ProfileRepetitionSummary::merge` and `ProfileTotals::record` maintain separate aggregation policies.

Create named `ProfileProjectRun` and accumulator types that own initialization, measured-versus-warmup handling, report digest/count updates, and merge behavior. Keep deterministic project ordering and repetition boundaries explicit, but route all aggregation through one implementation so a new timing or count field cannot be added to only one of the three paths.

Implemented `ProfileProjectRun` and `ProfileProjectRunAccumulator` to own loader-project initialization, warmup exclusion, repetition timing, report counts, evidence digests, phase totals, and error capture. `ProfileWorkloadSummary::merge`, `MeasuredRepetitionAccumulator::with_repetitions`, and `ProfileSummaryAccumulator` now centralize the remaining workload and suite aggregation boundaries while preserving deterministic ordering and repetition semantics.

### Provider and public-surface consistency

#### [x] READ-009 — Pure-data rule catalogs still use long fluent construction chains

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-js/src/rules/node/archive_compression/mod.rs:8-34`; `glass-lint-js/src/rules/js/header_indicator/mod.rs:12-63`; `glass-lint-obsidian/src/rules/platform/branching/mod.rs:11-64`; `glass-lint-obsidian/src/rules/vault/adapter/mod.rs:11-29`

Several provider catalogs still encode rows as long chains of repeated `Category::new`, `.query`, and `.build` calls, including a file with a `too_many_lines` suppression. These declarations are primarily static data, so the fluent form makes the catalog harder to scan and creates more repeated syntax than the existing rule metadata and query abstractions require.

Represent uniform rows with typed static slices or a small declarative row helper, then convert the rows at the catalog boundary. Keep fluent builders for exceptional rules with custom predicates or non-uniform evidence, and preserve explicit rule ordering and stable IDs in the catalog tests.

Implemented by moving the uniform module imports, header markers, platform members, and vault adapter methods into typed static slices consumed through `RuleBuilder::queries`. The archive catalog uses a small typed import-spec helper to retain its mixed exact/package semantics and original order; the header rule keeps its exceptional fetch-options query fluent and only data-drives the uniform literal rows.

#### [x] READ-010 — Project load metrics expose a mutable representation instead of a snapshot boundary

- **Severity:** Low
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-project/src/loader.rs:77-211,228,251,477-479,596-619,783-784`; `glass-lint-harness/src/profile/runner.rs` metric consumers

`ProjectLoadMetrics` exposes public timing and counter fields while its nested `ProjectPhaseTimings` has its own recording API and `AddAssign` implementation. Loader and harness code mutate the representation directly, which permits callers to construct combinations of timings, file counts, request counts, edges, and bytes that do not correspond to a real load and leaves aggregation policy split between the DTO and its producers.

Keep a private `ProjectMetricsAccumulator` for recording and merging, and publish `ProjectLoadMetrics` as an immutable snapshot with named accessors. Move `AddAssign` and recording methods to the accumulator, and have the loader perform one checked conversion at the outcome boundary; preserve the snapshot’s timing and counter values for harness consumers without allowing callers to fabricate partially inconsistent load results. If downstream users require construction, provide a validated constructor or a separate wire DTO rather than restoring public mutable fields.

Implemented with a private loader-side metrics accumulator and timing accumulator that own all recording and saturating merge operations. `ProjectLoadMetrics` and `ProjectPhaseTimings` now expose immutable snapshots through named accessors; the harness converts those snapshots into its own profile aggregation type, preserving phase totals and counter observations without mutating loader DTOs.

#### [x] READ-011 — The output crate re-exports the entire core project namespace

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** API
- **Location:** `glass-lint-output/src/lib.rs:8-13`

The output crate’s `project` module glob-re-exports `glass_lint_core::project::*` and its types, making the presentation crate a second import surface for every core project API. This couples output’s public API to unrelated core project additions and obscures which crate owns the project model, contrary to the separation between core report semantics and presentation adapters.

Replace the glob re-export with an explicit list of presentation-facing types, or remove the facade if consumers can import the core project API directly. Treat any retained re-exports as a deliberate compatibility contract and add an API test or documentation explaining which types are supported.

Implemented by removing the output crate’s `project` and nested `types` facades entirely. Its renderer now imports the core project types from `glass_lint_core::project`, leaving the core crate as the sole owner and public import surface for project models; repository call-site review found no consumer that required compatibility re-exports.

#### [x] READ-012 — Dead-code allowances mix live migration artifacts with legacy leftovers

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/module_request.rs:90-93`; `glass-lint-core/src/analysis/trace.rs:89-92`; `glass-lint-core/src/api/rule/query/limits.rs:1-13`; `glass-lint-core/src/api/rule/query/mod.rs:79-98`; `glass-lint-core/src/api/rule/query/error.rs:96-101`; `glass-lint-project/src/tsconfig/mod.rs:273-277`

The current tree has several narrowly scoped `dead_code` or `unused_imports` allowances, but call-site review shows they are not all dead. `QueryPredicate`, `VarType`, the query limit constants, `QueryDiagnostic::code/message`, and `TraceArena::is_exhausted` are used by compiler, runtime, CLI, harness, or test code; the allowances are stale migration leftovers, while `RecognizedModuleRequest::call_span` has no production accessor caller and the tsconfig re-export is only a test convenience inside a private module.

Remove the stale allowances from live symbols rather than deleting those symbols: the later implementation should first run the same workspace call-site check, then make the compiler enforce their usage normally. Treat `call_span()` as legacy unless a concrete reporting or matching consumer is identified; remove the accessor, and then reassess whether the stored span itself is needed for wrapping or equality. Replace the production tsconfig re-export with direct `selection::` imports and/or a test-only import, and delete it unless an external crate contract is found; keep any genuinely reserved API only with an owner, a documented activation condition, and a focused test.

Implemented by removing the unused `RecognizedModuleRequest::call_span` accessor and its unconsumed stored span, removing stale allowances from live query limits, compiler predicates, diagnostics, and trace state, and routing diagnostic formatting through the retained accessors. The tsconfig selection types are now reached through the crate-private `selection` module, with tests importing that module directly; no production facade remains.

## Systemic Themes

- Several earlier refactors introduced owner types, but their storage views and raw collection adapters remain public or cross phase boundaries. The next readability gains will come from completing those boundaries rather than adding more wrapper names.

- Core analysis is generally decomposed into recognizable phases, but the projector and summary/linking paths still coordinate multiple state machines through one mutable owner. Typed phase outcomes and narrower session objects would make the existing architecture easier to maintain without adding AST traversals.

- Bounded, deterministic analysis is a strong repository invariant. Resource limits, evidence capacity, path identity, and worklist termination should therefore be represented as domain types or named policy objects instead of parallel counters, tags, and raw indices.

- Provider rule catalogs have a consistent semantic model but inconsistent declaration syntax. A small data-oriented representation should be applied selectively to uniform rows while leaving genuinely semantic rules readable as code.

## Open Questions

The following decisions are recorded for the implementing agent; they are no longer requests to choose between equally preferred designs.

- **Path storage:** Make `ParentPathStore` reusable through a safe API. Keep the primitive available if core needs it, but hide raw tags, trusted depth, unchecked cross-store identity, and backing-store access. Prefer opaque handles plus checked operations and a single owner for frozen/overlay dispatch.

- **Dead-code triage:** Do not delete symbols solely because they carry `#[allow(dead_code)]`. The live query/compiler/runtime items identified in READ-012 should keep their behavior and lose stale suppressions; investigate and remove the unused `call_span()` accessor and test-only tsconfig re-export unless a real consumer or active migration is found. A future-facing item may remain only with an identified owner, activation condition, and test.

- **Output facade:** Treat the output crate’s `project` glob re-export as unsupported unless an external consumer is demonstrated. Remove it or replace it with an explicit, documented compatibility list; do not allow new core project APIs to flow through the output crate automatically.

- **Project metrics:** Use a private mutable accumulator and an immutable public snapshot. Preserve downstream observation and profiling fields through accessors or a validated constructor, but do not preserve public mutable fields merely for convenience.

## Coverage

- Read the root `ARCHITECTURE.md`, `TESTING.md`, `CONTRIBUTING.md`, and the architecture guide for each workspace crate before reviewing findings.

- Inventoried the Rust workspace (approximately 80,000 lines across nine crates), then inspected the largest and highest-risk modules in facts, scope building, matching, flow projection, summary propagation, project discovery/loading, profiling, and provider catalogs. Searches also covered public fields/functions, raw nested vectors, `Rc`/`Arc`/interior mutability, lint suppressions, TODO markers, and repeated collection/index operations.

- Ran `cargo clippy --workspace --all-targets --all-features -- -D warnings` successfully against the current tree. Clippy cleanliness was treated as validation of the review environment, not as evidence that the maintainability findings are absent.

- Ran `cargo test --workspace` successfully, including workspace unit tests and doc tests. The passing tests confirm the reviewed tree remains behaviorally green; they do not invalidate structural readability findings.

- No source, test, configuration, dependency, or generated project files were changed by this audit; this report is the only intended worktree modification.
