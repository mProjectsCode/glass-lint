# Glass Lint Core and Project Readability Audit

Audit date: 2026-07-23

Scope: the entirety of `glass-lint-core` and `glass-lint-project`, with emphasis on duplication, duplicated logic, borrowing, allocation and cloning, simplification, organization, and visibility of pipeline phases.

## Summary

This audit found 34 actionable readability and maintainability issues: 3 high severity, 25 medium severity, and 6 low severity. The highest-risk findings are local-flow budget exhaustion that is silently converted into missing evidence, non-canonical `tsconfig` cycle detection, and project resolver errors that are collapsed into ordinary missing-module outcomes. Scope planning and source-order collection intentionally remain separate passes; the narrower concern is duplicated structural traversal policy, not the existence of two traversals.

The broad architectural opportunity is to make each pipeline transition consume one phase-owned type and produce the next. Today, several boundaries retain raw and compiled forms together, erase semantic newtypes and reconstruct them later, or build parallel maps that describe one logical record. Those choices obscure the intended pipeline and cause avoidable clones, repeated validation, repeated indexing, and transient collections.

## Findings

### Core: Pipeline Architecture and Ownership

#### READ-001 — Two intentional scope passes duplicate their structural grammar
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/plan.rs:84-340`, `glass-lint-core/src/analysis/scope/collect/mod.rs:415-687`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:31-269`

The declaration planner must finish before source-order provenance collection so hoisted and TDZ bindings are known at every use position. The issue is therefore not that the AST is traversed twice; it is that `ScopePlanner` and `ScopeCollector` independently enumerate scope-forming syntax and its child order, so adding or changing support for a function, arrow, block, loop, switch, `with`, or catch scope requires synchronized edits.

**Historical constraint:** Commit `f14cf0f` previously combined the phases in one `LexicalScopeCollector` with a `Pass::Predeclare`/`Pass::Collect` mode. That design still traversed the AST twice, kept source-order state alive during predeclaration, spread pass-conditioned behavior across the collector, and required divergence/reuse modes in the same mutable owner. Commit `d101254` intentionally restored a consuming `ScopePlan` boundary and distinct `ScopePlanner` and `ScopeCollector` state; that separation should not be reverted.

**Decision:** Keep declaration planning, source-order collection, and freezing as three explicit phases. Do not merge the planner and collector, do not restore a `Pass` enum, and do not make one phase stateful object mutate itself from planning mode into collection mode. This finding should be implemented only as consolidation of the shared traversal grammar; leaving the current two visitors is preferable to a design that weakens phase ownership or introduces `Rc`, `RefCell`, unsafe aliasing, or clone-to-release-borrow workarounds.

**Implementation guidance:**

1. First add a test-only structural trace emitted by both current visitors. Each trace entry should contain the parent scope occurrence, scope kind, complete span, and a sibling occurrence number; assert exact trace equality before any refactor. Keep the existing `ScopeShapeTable` mismatch and unconsumed-shape tests because the production path must remain fail closed even after the trace test exists.
2. Introduce a phase-neutral `ScopeTraversal<P>` whose `P` is an owned phase policy, with separate `PlanningScopePass` and `CollectionScopePass` implementations. The traversal, not either pass, should be the sole owner of the `Visit` methods for the overlapping structural syntax. Running `ScopeTraversal<PlanningScopePass>` and then `ScopeTraversal<CollectionScopePass>` still performs two AST walks and produces a consuming `ScopePlan` between them.
3. Keep phase storage disjoint. The planning policy may own only names, lexical scopes, declaration bindings, structural identities, and exhaustion state. The collection policy may consume the immutable plan and own only source-order assignments, aliases, calls, mutations, callback facts, dynamic-eval effects, and mismatch diagnostics.
4. Give the traversal explicit hooks around children rather than a general callback that can recursively visit arbitrary nodes. The traversal must define child order once, while hooks perform phase-specific work at named points such as declaring a parent binding, entering a planned scope, declaring parameters, observing an initializer, and leaving the scope. A hook must not call `visit_with` itself, because that would restore two authorities over nesting order.
5. Migrate one syntax family at a time while both old visitors still compile: plain blocks first, then loops and switch, then catch and `with`, and functions/arrows last. After each family, remove the corresponding old `Visit` methods immediately and run the focused scope tests; do not leave delegating old methods as a compatibility layer.
6. Preserve the current non-obvious order exactly. A function declaration name is declared in its parent before entering the function scope; parameters belong to the function scope; a catch parameter and its body statements share the catch scope without adding a second body-block scope; a switch discriminant is visited before the switch scope; a `with` object is visited before entering the dynamic scope; loop initializers/tests/updates execute within the loop scope; and `var` walks outward to the nearest function/program owner while `let` and `const` remain lexical.
7. Do not identify scopes by span alone. Retain a structural identity containing parent `ScopeId`, kind, full span, and deterministic sibling occurrence, or preserve the current per-key queue with an explicit occurrence in the trace. Equal-span/generated nodes must consume distinct planned scopes deterministically.
8. On any plan/collection mismatch, never allocate a fallback scope and never continue resolving globals as if the plan were valid. Record `ShapeMismatch`, keep provenance local/unknown for the affected path, detect unconsumed planned shapes during freeze, and propagate the existing incomplete-analysis status.
9. Require parity tests for hoisting, TDZ use-before-declaration, `var` across nested blocks, named function/class declarations, default/rest/destructured parameters, imports, catches, every loop form, switch, `with`, direct and aliased `eval`, nested functions/arrows, equal-span synthetic nodes, minified source, name exhaustion, and unsupported/dynamic patterns. Add one structural test fixture containing every scope-forming syntax so a missing traversal arm fails in a single focused test.
10. Completion means there are still two invocations and two phase-owned state types, but only one production implementation of the overlapping scope-forming `Visit` methods. Benchmark representative large/minified files and reject the refactor if the generic driver increases peak memory materially, requires cloned AST nodes, or makes the traversal order harder to audit than the current duplication.

If `ScopeTraversal<P>` cannot be expressed with ordinary exclusive borrowing and narrow hooks, close this finding as accepted duplication and retain the current plan/collector design. A macro that generates two visitors is a fallback only if its declaration makes child order readable and adding a new scope-forming syntax cannot compile without defining both phase hooks.

#### READ-002 — Local-flow resource exhaustion silently becomes absent evidence
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/index.rs:14-49`, `glass-lint-core/src/analysis/flow/projector/state.rs:25-126`, `glass-lint-core/src/analysis/flow/projector/mod.rs:39-76`

Local flow uses hard-coded limits for objects, states, emissions, and mutation-log entries. When those limits are reached, paths are dropped or restoration fails without a typed outcome, so callers cannot distinguish “no flow” from “analysis exhausted”; cross-module flow already treats exhaustion as a first-class partial-analysis condition.

Return a `LocalFlowProjectionOutcome` containing evidence, exhaustion state, and bounded counters. Derive all flow budgets from the validated analysis limits; any local-flow exhaustion should record a file-scoped flow-budget diagnostic and make the combined report `Partial`, matching facts, effects, linking, and cross-module flow. Add focused tests for each limit so exhaustion consistently fails closed instead of looking like a clean negative result.

#### READ-003 — The link graph retains metadata that does not drive linking
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/state.rs:10-76`, `glass-lint-core/src/analysis/project/graph.rs:27-80`

The link graph stores SCC components and a provenance map, but the global-export fixed point iterates the full export set rather than the component graph, and provenance has no production reader. SCCs are used for a size check and then retained through matching, while the architecture documentation describes SCC-driven convergence.

Treat SCCs as a transient validation result used only to enforce `MAX_SCC_SIZE`; do not retain the component vectors in `ModuleGraph` after that check. Remove edge provenance now because there is no reader, and reintroduce it only with the diagnostic that consumes it. Correct the architecture comment to describe the current bounded global fixed point; an SCC-DAG linker should be a separate measured optimization, not bundled into this cleanup.

#### READ-004 — Validated project input is represented as parallel maps and a positional tuple
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/input.rs:101-190`, `glass-lint-core/src/analysis/project/model.rs:104-161`

`ValidatedProjectInput` keeps sources, resolutions, module IDs, and request IDs in separate maps keyed by overlapping identities, then exposes them as a five-element tuple. The linking phase uses only part of that payload, while source text and root information remain attached beyond the phase that owns them.

Introduce named, consuming phase types such as `AdmittedProject`, `LocallyAnalyzedProject`, and `ResolvedLinkInput`. Store each resolution beside its qualified request identity instead of maintaining synchronized key spaces. Split report file-roster data from linker input so source text can be dropped or borrowed as soon as local analysis finishes.

#### READ-005 — Direct project linting assigns identities and then rebuilds the project
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lint/linter.rs:192-213`, `glass-lint-core/src/project/input.rs:40-89`, `glass-lint-core/src/project/session.rs:785-833`

`Linter::lint_project` validates `ProjectInput` and computes module/request IDs, then discards those maps and re-admits the same data through `ProjectCollection`. Session resolution subsequently recomputes identities and revalidates outcomes, leaving two project-construction paths with overlapping responsibilities.

Keep `Linter::lint_project(ProjectInput)` as the public convenience API documented by core, but make it a thin adapter into the same staged session pipeline. Its initial admission should normalize the root and sources and enforce DTO size/duplicate constraints without assigning module/request IDs; after local analysis discovers the authoritative request set, `LocallyAnalyzedProject::resolve` should validate outcomes and assign both identity tables exactly once. Delete the early ID maps and the second normalization path rather than maintaining a separate direct-input linker.

#### READ-006 — Authored requests are materialized and projected more than once
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/session.rs:315-359`, `glass-lint-core/src/project/session.rs:785-828`, `glass-lint-core/src/analysis/module.rs:320-357`

Local artifacts store a `BTreeMap` whose value repeats the complete request even though later code primarily needs membership. Resolution then calls `requests_with_ids` again, recreating request strings and keys to recover local IDs; the module interface also converts typed importer paths to strings and rebuilds them.

Build an `AuthoredRequestTable` once per module, containing the local request ID and the public request record. Keep a key-to-ID index for validation and qualify it once after module IDs are assigned. Expose borrowed iteration internally and reserve owned request conversion for the API boundary.

#### READ-007 — Normalized path and resolution newtypes are repeatedly erased and revalidated
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/project/types/mod.rs:29-67`, `glass-lint-core/src/project/input.rs:59-79`, `glass-lint-core/src/project/session.rs:649-684`

Already validated `ProjectRelativePath` and resolution values are converted to `&str` or `String` and then reconstructed through validating constructors. `SourceTable::iter` exposes string paths, pending work owns another string copy, and report assembly rebuilds normalized paths instead of cloning the typed identity.

Carry semantic newtypes through internal iterators, queues, and report builders. Restrict raw-string validation to constructors and deserialization, then trust the established invariant. Where an owned queue is needed, drain or cheaply clone the path newtype rather than cloning its text and reparsing it.

#### READ-008 — Parallel lowering discards and reconstructs source location data
- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/lowering.rs:139-155`, `glass-lint-core/src/project/session.rs:181-219`, `glass-lint-core/src/project/session.rs:686-699`

Lowering constructs a `SourceLineIndex`, but the parallel worker projects the result down to semantic data and later reconstructs the line index from the source. Consuming a lowered source also clones its source context into the local artifact instead of moving the complete phase result.

Carry `LoweredSource` through the worker result and move its source context into the next phase. If workers intentionally produce semantics only, split semantic lowering from source-location attachment so the first index is never constructed. Make the worker output type name the actual phase boundary.

#### READ-009 — The local artifact cache is invalidated by non-local budgets
- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/local.rs:30-75`, `glass-lint-core/src/analysis/local.rs:102-168`, `glass-lint-core/src/project/tests/cache_and_session.rs:181-265`

The cache fingerprint includes the full `AnalysisLimits`, including evidence, linking, and flow budgets that do not affect matcher-independent parsing and semantic facts. Tests currently encode cache misses for these unrelated changes, coupling the local artifact phase to downstream execution policy.

Define a `LocalLoweringConfig` or fingerprint containing only syntax, semantic, and fact-building inputs. Apply projection and link budgets after the cached boundary. Update cache tests to require hits when only downstream limits change and misses only when local semantics can differ.

### Core: Facts, Flow, Matching, and Linking

#### READ-010 — Function and call metadata is copied across facts, effects, and summaries
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/model.rs:192-305`, `glass-lint-core/src/analysis/flow/effect.rs:342-469`, `glass-lint-core/src/analysis/flow/summary.rs:279-400`

Parameter bindings and call relationships originate in semantic facts, are cloned into function effects, and are copied again into function summaries. These representations coexist within one analysis lifetime, so the extra ownership obscures which structure is canonical and increases memory churn for call-heavy files.

Keep canonical function and call tables addressed by typed IDs. Let effects and summaries store IDs plus only genuinely derived state, borrowing immutable metadata during projection. If summaries must outlive facts, make that ownership transition explicit and consuming rather than cloning at every intermediate phase.

#### READ-011 — Flow symbols are rebound repeatedly instead of once per module
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/index.rs:85-113`, `glass-lint-core/src/analysis/flow/projector/evidence.rs:25-139`, `glass-lint-core/src/analysis/flow/summary.rs:605-660`, `glass-lint-core/src/analysis/flow/cross/mod.rs:338-460`

Sources are converted to module-local name paths for indexing, but requirements and sinks are converted again inside event/state loops, and cross-module propagation reconstructs owned symbol paths. Cross-flow candidate discovery also scans every flow for each effect/call instead of sharing the source index used by local projection.

Add a `BoundFlowPlan` phase between catalog compilation and flow execution. Bind sources, requirements, sinks, and present-argument indexes to module-local identities once, then share those indexes between local and cross projection. Return iterators for small index sets and keep owned `SymbolPath` values only in the provider-neutral catalog.

#### READ-012 — Flow-state edits clone full states and log unchanged mutations
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:304-416`

State mutation clones the prior state, then clones the new state on guard drop even if nothing changed. Object removal clones matching entries before cloning again into the mutation log, while joins clone and clear whole alias/state collections before reinsertion.

Make the edit guard compare old and new values and skip no-op log entries. Use ordered-range removal or draining for one object's states, and merge into scratch storage rather than repeatedly clearing and reinserting. Consider shared or persistent state values only after measuring; the first guardrail is to stop unconditional full-state snapshots.

#### READ-013 — Evidence is allocated per occurrence and immediately regrouped
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/matching/arguments.rs:200-246`, `glass-lint-core/src/analysis/matching/mod.rs:391-416`, `glass-lint-core/src/analysis/evidence.rs:30-112`

Matching creates an owned evidence value, including a cloned symbol, for each occurrence. The evidence accumulator then drains these values and groups them again by kind and symbol, causing repeated ownership transitions for data that is naturally produced as a group.

Accumulate locations directly under a borrowed or compiled evidence descriptor and own the symbol once when finalizing the group. Give ordinary, constrained, and flow matching a common bounded evidence sink. Preserve deterministic location order and truncation at the sink boundary.

#### READ-014 — Semantic-fact projection builds two full evidence matrices
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:145-178`

Projection allocates one per-rule evidence matrix, calls object-flow collection to allocate another matrix of the same shape, and then extends the first from the second. Peak memory therefore includes both complete collections even though they share a final destination.

Pass a shared bounded evidence sink into both projectors, or return movable phase batches that can be merged without parallel outer matrices. Keep rule-order determinism in the accumulator rather than relying on a final append pattern. Make the memory budget apply to the combined producer set.

#### READ-015 — Export storage clones flat keys and namespace resolution retraverses exports
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/state.rs:80-170`, `glass-lint-core/src/analysis/project/identities.rs:76-148`

The export table uses `(ModuleId, SmolStr)` flat keys, requiring owned name construction for common lookups and updates. Namespace resolution recursively collects exported names into temporary sets and then performs another recursive lookup for those names, repeating graph traversal and allocation.

Store exports as `ModuleId -> ModuleExports` so module lookup and borrowed name lookup are distinct operations. Expose a deterministic iterator over the resolved export table and use it for namespace projection. Cache only at a phase boundary with a clear invalidation rule; do not add another parallel export model.

#### READ-016 — Matched capabilities clone catalog metadata for every module
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/model.rs:454-489`, `glass-lint-core/src/lint/report.rs:145-258`

Each transient `MatchedCapability` owns rule ID, description, and category strings even though report assembly already has the immutable catalog and mostly consumes the rule index, label, severity, and evidence. Repeating static rule metadata per matched module increases allocations and creates another representation that can drift from the catalog.

Keep internal matches as rule IDs/indexes plus evidence and any result-specific label or severity. Let report assembly borrow catalog metadata while constructing the owned public report. If a standalone classification result is public API, make its ownership conversion an explicit final step.

#### READ-017 — `Linter::clone` is documented as cheap but deep-clones its execution plan
- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** API
- **Location:** `glass-lint-core/src/lint/linter.rs:55-81`

Cloning a linter duplicates the catalog records, compiled plans, and enabled-rule vector; only the cache/environment handles are naturally cheap. The API documentation therefore understates the cost for large catalogs and encourages cloning at a boundary where shared immutable state is appropriate.

Treat cheap cloning as part of the public concurrency contract because the type-level documentation explicitly promises it. Store the immutable catalog, compiled execution plan, enabled set, environment, and limits in one `Arc`-backed configuration object, while keeping the already shared artifact-cache handle separate. Add a test or size/identity assertion showing clones share the compiled configuration; do not merely weaken the documentation while retaining a deep `Clone`.

#### READ-018 — Query-plan compilation has separate production and test implementations
- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/rule.rs:241-336`

The test-only `QueryPlan::from_declarations` and production compilation both collect clauses and flows, sort/deduplicate them, and validate the result. The test path omits some package and flow validation, so tests can exercise a compiler-shaped implementation that is not the production compiler.

Extract one internal compilation function with explicit inputs for declarations and package context. Call it from both the public compiler and unit tests. Keep convenience fixture construction outside the query-plan type so validation cannot be bypassed accidentally.

#### READ-019 — CommonJS object-export properties are traversed and decoded twice
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/build/interface.rs:411-480`, `glass-lint-core/src/analysis/facts/build/interface.rs:517-546`

CommonJS object export handling first validates and materializes export entries, then loops over the object properties again for function/static metadata before committing the entries. The two passes duplicate property interpretation and make atomic fail-closed behavior harder to see.

Normalize each property once into a typed `CommonJsExportEntry` containing the export name, local/value identity, function metadata, and static value. Validate the complete object before mutating the interface, then commit the normalized entries in one deterministic pass. Keep unsupported properties as one explicit rejection path.

#### READ-020 — Frozen assignment history loses its owning abstraction
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/scope/model.rs:73-75`, `glass-lint-core/src/analysis/scope/model.rs:264-284`, `glass-lint-core/src/analysis/scope/model.rs:343-367`

Collection has an assignment-history concept, but the frozen scope model exposes it as nested maps. `assignment_at` and `binding_version` repeat nearly identical partition-point logic, while `reassigned_between` linearly scans an already sorted history.

Preserve a `FrozenAssignmentIndex` newtype with `latest_at`, `version_at`, and `changed_between` operations. Implement all range queries with one binary-search primitive and keep ordering validation at construction. This puts temporal-binding invariants on the state-owning type and removes duplicated callers.

#### READ-021 — Overlay matching allocates a bucket vector for each query
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/matching/query.rs:32-76`, `glass-lint-core/src/analysis/matching/occurrence.rs:1-90`

Module occurrence queries clone a `Vec<&[Occurrence]>`, and merged base/overlay queries allocate and copy bucket references before iteration. This happens at clause-query granularity even though the underlying occurrence slices are already stable and borrowed.

Represent iteration as an optional base slice plus a borrowed slice of overlay buckets. Make `BorrowedOccurrenceIter` borrow that view rather than own a newly assembled vector. Retain deterministic merge order without allocating a container per matcher query.

#### READ-022 — Event-index construction repeats a sparse ten-field record
- **Severity:** Low
- **Fix Complexity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/query.rs:83-95`, `glass-lint-core/src/analysis/matching/query.rs:388-485`

`EventIndexView` is a record of ten optional indexes, and event dispatch repeats large constructors whose main distinction is which one or two fields are populated. The legal field combinations are implicit in match arms instead of represented by the type system.

Use an enum for event families or a defaulted view with small family-specific constructors. Keep shared base and overlay mapping in one helper. Let compiler validation guarantee which event families a query can request so impossible combinations remain unrepresentable.

#### READ-023 — Session execution, caching, and phase state share one oversized module
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/session.rs:1-525`

Job executors, observer hooks, cache lookup, test-controlled execution, artifacts, and project phase types are interleaved in one module. `SessionState` owns the artifact cache while `ProjectCollection` clones another handle solely to perform local work, which blurs whether caching belongs to the session runtime or collection phase.

Split the module into execution runtime, local artifact production, and project phase-state modules. Keep one cache owner, or introduce an explicit `LocalExecutionRuntime` borrowed by collection methods. Arrange the public state transitions together so collection, local analysis, resolution, and finalization are apparent from the file structure.

#### READ-024 — Consuming `EvidenceList` clones locally owned evidence
- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/tables.rs:159-167`

`IntoIterator for EvidenceList` delegates to borrowed iteration and clones every item into a temporary collection. That is necessary for shared backing storage but unnecessarily clones evidence already held in the local owned vector.

Provide a custom owning iterator that moves local entries and clones only entries backed by shared immutable storage. If the mixed representation makes that contract surprising, expose an explicit consuming flatten operation instead of `IntoIterator`. Preserve the current deterministic order between shared and local segments.

#### READ-025 — Deterministic FNV hashing is duplicated
- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/local.rs:30-37`, `glass-lint-core/src/environment.rs:301-325`

The same byte loop and FNV prime are implemented in local-analysis fingerprinting and nested environment fingerprint helpers. The duplication is small, but these fingerprints participate in cache correctness and should not acquire subtly different framing or constants.

Introduce a small deterministic fingerprint writer with typed methods for bytes, strings, integers, and sequence framing. Use it for both fingerprints and document stability expectations. Keep it private to core unless the serialized fingerprint becomes a cross-crate contract.

#### READ-026 — Module export metadata is split across parallel maps
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/module.rs:158-171`, `glass-lint-core/src/analysis/module.rs:212-263`

`ModuleInterface` keeps export resolution, exported functions, and exported static strings in separate maps keyed by the same name. Conflict and unknown-export operations update only subsets of these maps, so consistency depends on callers remembering which metadata is meaningful after resolution changes.

Model one `ExportEntry` containing resolution plus optional function and static metadata. Apply conflicts and unknown degradation atomically on that entry. Expose focused queries so consumers cannot observe auxiliary metadata without also checking the export state.

### Project: Discovery, Resolution, and Configuration

#### READ-027 — Validated extension normalization is bypassed during admission
- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-project/src/options.rs:188-218`, `glass-lint-project/src/options.rs:291-338`

Validation builds a lowercased `SourceExtensionSet`, but `ValidatedProjectLoadOptions::supports` delegates to the raw options method, which lowercases the path and every configured extension on each admission check. The normalized set is used by resolution but not by the discovery boundary that performs the same policy.

Put suffix support and exclusion on `SourceExtensionSet`, and make validated options delegate only to that normalized owner. Validate/build once and keep raw options out of runtime admission. Add mixed-case extension tests at the normalized-set boundary.

#### READ-028 — `tsconfig` cycle detection compares canonical and unresolved paths
- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig.rs:487-551`

The current configuration is canonicalized before it is added to the extends chain, but the prospective parent path is checked for membership before parent canonicalization. Equivalent paths containing `..`, alternate lexical spellings, or symlink aliases can therefore evade cycle detection and recurse until another resource limit intervenes.

Resolve and canonicalize the parent before cycle comparison and recursion. Track canonical paths in a set for membership and a stack for deterministic diagnostics. Treat canonicalization failure as an explicit configuration error rather than changing identity semantics.

#### READ-029 — `tsconfig` parsing, inheritance, and selection remain one clone-heavy representation
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-project/src/tsconfig.rs:242-321`, `glass-lint-project/src/tsconfig.rs:487-562`

Parent configs are owned by the recursive loader but passed by reference into construction, which clones inherited file/include/exclude data. DTO `extends` and references are also cloned, and the resulting config retains raw include/exclude strings beside compiled pattern sets even though production selection uses the compiled form.

Separate `ParsedTsconfig`, consuming inheritance, and `CompiledTsconfigSelection` phases. Move owned parent and DTO fields when constructing the child, compile effective patterns once, and discard raw selection text unless diagnostics require it. If tests need the parsed form, test that intermediate type rather than retaining duplicate production state.

#### READ-030 — Resolver and loader collapse filesystem errors into missing modules
- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/resolver.rs:67-93`, `glass-lint-project/src/loader.rs:533-548`

`ProjectResolver::classify` maps every admission I/O error to `ResolverOutcome::Missing`. Loader paths also use `exists()` and `if let Ok(...)`, silently discarding metadata, canonicalization, or admission failures and making boundary errors indistinguishable from a genuine absent candidate.

Return `Result<ResolverOutcome, ProjectLoadError>` through resolution caching and frontier expansion. Reserve `Missing` for a confirmed absent or unsupported candidate and preserve the offending path/source error for failures. Decide at the top-level loader whether an error aborts loading or produces an explicitly partial project.

#### READ-031 — Admitted paths do not carry their root-relative identity
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-project/src/admission.rs:31-175`, `glass-lint-project/src/discovery.rs:65-115`, `glass-lint-project/src/discovery.rs:206-236`

`AdmittedSourcePath` wraps only a canonical path, even though successful admission establishes containment within one project root. Downstream code recomputes containment, uses a fallback when stripping the root, and prechecks support before calling admission, which repeats parts of the boundary policy.

Have the root-owned admission service produce an `AdmittedSource` containing canonical and validated project-relative identities. Remove downstream containment rechecks and the `unwrap_or(path)` fallback because the type should make that state impossible. Let one admission operation handle support, metadata, canonicalization, and containment exactly once.

#### READ-032 — Phase timing names do not match the measured work
- **Severity:** Low
- **Fix Complexity:** Medium
- **Category:** Naming
- **Location:** `glass-lint-project/src/loader.rs:50-159`, `glass-lint-project/src/loader.rs:485-503`

The timing structure describes a different number of fields than it exposes, `parse_and_local_analysis` aliases `local_analysis`, and linking/matching is derived rather than directly measured. The local timer includes cache lookup, parse/lower work, and request projection, so its name does not reveal the boundary being timed.

Use a named phase enum or table whose values correspond exactly to loader transitions. Remove the compatibility-like alias, or split parse from local projection if the distinction is operationally meaningful. Derive totals in one place and document inclusive/exclusive timing semantics.

#### READ-033 — Documented `tsconfig` cycle behavior contradicts implementation
- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Other
- **Location:** `glass-lint-project/src/tsconfig.rs:464-470`, `glass-lint-project/src/discovery.rs:142-144`

Comments describe a fail-closed sentinel configuration that excludes all files on an extends cycle, while the implementation skips the cyclic parent and continues building the child/local configuration. The discrepancy makes it unclear which behavior callers and tests should rely on.

The existing tests establish the intended contract: emit a deterministic diagnostic, discard only the offending cyclic `extends` edge, and retain the current file's local settings plus any already resolved acyclic ancestors. Update the comments to state that behavior and add a canonical-alias cycle case alongside the current lexical cycle tests. Do not restore a sentinel config, exclude the entire project, or silently accept the edge.

### Cross-Cutting Organization and Tests

#### READ-034 — Large inline test modules hide production phase structure
- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Testing
- **Location:** `glass-lint-core/src/analysis/scope/collect/mod.rs:754-1254`, `glass-lint-core/src/project/report.rs:105-481`, `glass-lint-project/src/tsconfig.rs:566-864`

Several already-large production modules include hundreds of lines of inline tests, with similar concentrations in fact building, resolution, and flow projection. Private access is convenient, but the resulting files make the production state transitions and ownership boundaries harder to scan.

Move substantial test bodies into sibling `tests.rs` modules while retaining private access through the parent module. Keep very small invariant tests inline when proximity materially helps. Organize the extracted tests by behavior and adversarial case, not by source function name.

## Systemic Themes

- **Make phases consume ownership.** Parsed, validated, locally analyzed, resolved, linked, and reported data should be different types. Consuming transitions would eliminate several parallel maps, raw-plus-compiled fields, and clones whose only purpose is preserving an earlier phase.
- **Keep semantic identities typed.** Paths, module IDs, request IDs, function IDs, and export entries should remain typed inside the pipeline. Reconstructing them from strings obscures invariants and repeatedly pays validation and allocation costs.
- **Bind immutable policy once.** Catalog queries and flow declarations should be compiled once globally and bound once per module. Hot matching and propagation loops should operate on IDs and borrowed slices, not reparse symbol paths or recreate requests.
- **Make bounded failure observable.** Every exhausted budget should produce a typed, deterministic partial-analysis outcome. Silent truncation is both a correctness risk and an architectural leak.
- **Store one logical record once.** Parallel maps for exports, project input, and request metadata invite drift and inhibit borrowing. Cohesive records behind owner types make invariants enforceable and reduce key cloning.
- **Let transient structures be transient.** Line indexes, graph components, provenance, raw `tsconfig` patterns, and intermediate evidence matrices should be consumed or discarded at their owning boundary unless a downstream feature demonstrably uses them.

## Open Questions

None remain after tracing the current status model, public documentation, tests, consumers, and the Git history of the scope collector:

1. **Local-flow exhaustion makes the report partial.** This matches every other semantic/linking budget and prevents an exhausted negative result from being presented as complete. Record the reason at file scope for the affected module and let normal report combination raise project completion to `Partial`.
2. **SCCs are transient validation data; edge provenance is currently dead data.** Compute SCCs, enforce `MAX_SCC_SIZE`, retain only the metric/status needed downstream, and discard the components. Remove edge provenance until a concrete diagnostic consumes it; describe the present export algorithm as a bounded global fixed point rather than claiming SCC-driven convergence.
3. **`Linter::lint_project(ProjectInput)` remains a public convenience facade, not a second internal pipeline.** Its documented API is useful for in-memory callers, but it should feed the staged collection/local/resolution pipeline before IDs are assigned. Module and request identities should be created once, after the authoritative source and authored-request sets exist.
4. **A `tsconfig` extends cycle drops only the offending edge.** Emit a deterministic diagnostic, retain the current config's local settings and previously resolved acyclic inheritance, and do not broaden selection with the cyclic parent. This is the behavior asserted by the existing cycle tests; canonicalizing the candidate parent before membership checks closes the remaining alias-path hole.
5. **Cheap `Linter` cloning is a public contract.** The type documentation promises it for concurrent use, so immutable compiled configuration should be shared with `Arc`; only runtime handles with intentionally different sharing semantics should remain separate.

## Coverage

The audit enumerated and inspected all Rust source modules in `glass-lint-core` and `glass-lint-project`, including inline and dedicated test modules. Core coverage included API/catalog compilation, parsing/lowering, scope and semantic facts, matching, local and cross-module flow, project identities/input/linking/reporting, session/cache/execution, lint assembly, environment, diagnostics, and limits. Project coverage included options, admission, discovery, source loading, resolution, module path handling, `tsconfig` parsing/inheritance/selection, loader orchestration, corpus assembly, profiling, and tests.

Repository-level and owning-crate architecture documents, `TESTING.md`, `CONTRIBUTING.md`, manifests, and the prior report history were reviewed for intended boundaries and already-resolved findings. `cargo clippy -p glass-lint-core -p glass-lint-project --all-targets -- -W clippy::pedantic` completed successfully; its remaining output was predominantly documentation and `must_use` suggestions and was not duplicated here as readability findings.
