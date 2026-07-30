# `glass-lint-core` Readability, Performance, and Technical-Debt Audit

## Summary

This audit covers the entire `glass-lint-core` source tree and its core-owned tests, with extra attention to the query API/compiler and flow-matcher changes in the commits from `6e64414` through `0665519`. It found 25 active issues: 9 High, 15 Medium, and 1 Low severity.

The most urgent correctness problems are flow alternatives being deduplicated by a bare 64-bit hash, the flow evidence cap not actually capping total emissions, ternary branches sharing or leaking fact-builder origin state, and conjunction validation accepting impossible predicate sets. The largest hot-path costs are three full AST visits in `lower_program`, scope joins that still clone complete environments, unconditional local-flow setup for catalogs with no lifecycle matchers, flow tables that repeatedly scan all live states, and cross-call seeding that forms a call-argument × flow Cartesian product.

Several recent changes improved ownership and removed earlier clones, but some optimizations stopped one step too early. In particular, replacing full path snapshots with hash-only equality introduced a soundness regression, and adding a `BoundFlowPlan` did not make its entries executable enough to avoid reinterpreting every source, requirement, and sink in the transfer loop.

The recommended crate decisions are specific rather than blanket additions: use Rayon for project-local parallel execution; use the existing `smallvec` dependency for tiny transfer buffers; retain the existing `petgraph` SCC implementation; and do not add regex, glob, LRU, or persistent-collection crates for domain problems they do not solve. The scope and flow checkpoint structures need domain-specific delta/index redesigns, not a generic compatibility layer.

## Findings

### High severity

#### [x] READ-001 — Flow joins use a hash as semantic identity

- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:551-583`, `glass-lint-core/src/analysis/flow/projector/state.rs:215-229`

`join_paths` drops a path whenever its 64-bit `DefaultHasher` fingerprint was seen, without an equality check. A collision can therefore combine distinct path-local alternatives, while computing the purportedly O(1) fingerprint still walks and hashes every alias and nested flow-state entry.

Restore collision-safe equality by storing canonical state IDs or hash buckets followed by full semantic equality. Cache or incrementally maintain the hash if profiling justifies it, but never let the hash alone establish semantic equivalence. Add a test hasher that deliberately collides so the equality fallback is exercised deterministically.

**Fix:** Flow joins now retain canonical semantic snapshots and compare them for equality, so distinct path-local states cannot be coalesced by a 64-bit hash collision. The flow-state unit tests also verify that distinct snapshots remain distinct.

#### [x] READ-002 — The global flow-evidence limit is bypassed for existing keys

- **Severity:** High
- **Fix Complexity** Low
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:321-390`

`FlowEvidence::try_insert` checks `total_emitted >= limit` only when the per-key count is zero. Once a key exists, up to 256 more traces for that key can be appended after the global limit, contradicting the method contract and allowing retained output and trace work to exceed the configured bound.

Check the global cap before every successful increment, while keeping the separate per-key cap. Track an explicit `limit_rejected` flag so exhaustion means an attempted operation failed, not merely that capacity was exactly filled. Add boundary tests with repeated emissions for one existing key and for several existing keys.

**Fix:** `FlowEvidence` now checks the global cap before every emission, including emissions for keys that already have retained traces, and records whether the cap rejected work. Boundary tests cover repeated emissions for one key and multiple existing keys.

#### [x] READ-003 — No-flow catalogs still run local flow setup and full-stream passes

- **Severity:** High
- **Fix Complexity** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:550-590`, `glass-lint-core/src/analysis/flow/projector/mod.rs:70-101`

`SemanticFacts::project` always calls `object_flow::collect_into`, even when `ProjectionPlan::flow_matchers` is empty. That empty case still binds a plan, constructs function summaries, builds `calls_by_result` with a full fact scan, constructs a projector, and transfers every fact, so ordinary call/member catalogs pay multiple flow passes per module.

Return immediately when `flow_matchers.is_empty()` and make that fast path observable in a focused test. Gate later setup by the already-computed `FlowRequirements` rather than rediscovering an empty plan inside the projector. Keep the guard at the owning projection boundary so every caller receives the same behavior.

**Fix:** Semantic projection now returns after ordinary constrained matching when no lifecycle matcher is selected, and the flow collector has the same defensive empty-input fast path. A focused projector test verifies that an empty flow catalog performs no flow operations.

#### [x] READ-004 — Scope checkpoints still clone whole environments at every join

- **Severity:** High
- **Fix Complexity** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/build/history.rs:121-135`, `glass-lint-core/src/analysis/scope/build/history.rs:218-299`, `glass-lint-core/src/analysis/scope/build/assignments.rs:127-245`

Checkpoints are O(1), but every restore allocates two complete root paths, every reachable join arm clones the full nested assignment map, the incoming map is cloned again, and `AssignmentEnvironment::join` scans every stored scope/name. `CollectorCheckpoint` also clones the complete `BTreeSet` of writes, so sequential branch-heavy files can approach quadratic work despite the new mutation log.

Store depth and compute the LCA by walking parent links as the flow mutation log already does. Join only branch-local touched bindings against the incoming value, and represent the write set as a checkpointed delta or dense/hash set with deterministic sorting only when freezing. Benchmark a domain delta design before considering `im` or `rpds`; a generic persistent map is not automatically a win for these short-lived, write-heavy environments.

**Fix:** Scope joins now transition the existing parent-linked assignment history and read only branch-touched bindings, avoiding complete environment snapshots. The write set is also checkpointed through generation-tagged deltas with deterministic sorting at the join boundary. Regression tests cover assignment restoration, branch-local write restoration, and the existing scope precision suite.

#### [x] READ-005 — Ternary branches leak and stitch fact-builder origins

- **Severity:** High
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/control.rs:188-215`, `glass-lint-core/src/analysis/facts/origin_map.rs:26-75`

`record_conditional` rolls `instance_origins` back after the consequent, which decrements and closes its only checkpoint; alternate-branch mutations are then unlogged, so the second rollback is ineffective. `class_origins` is not restored between the branches at all, allowing the alternate traversal to observe origins established only by the incompatible consequent.

Replace the raw `usize`/open-count protocol with an explicit branch transaction that captures incoming state, evaluates each arm independently, and joins only common proven origins. Make checkpoint close/commit single-use through a token or guard so a rollback cannot silently disable later logging. Add adversarial ternary tests for instance and class origins in both arm orders, including assignments and calls inside the alternate arm.

**Fix:** Origin-map checkpoints are now single-use transaction tokens with separate restore and close operations. Ternary arms restore both instance and class origins to the incoming state, then retain only origins proven in both arms. Tests cover incompatible instance and class assignments in both arm orders.

#### [x] READ-006 — Predicate contradiction checking is pairwise, not conjunctive

- **Severity:** High
- **Fix Complexity** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/contradiction.rs:58-188`

Exact sets are rejected only when a pair is disjoint, although three pairwise-overlapping sets can have an empty total intersection. Exact/prefix checking accepts when any exact value matches any prefix set, rather than requiring one candidate to satisfy every conjunct; incompatible prefix-only conjunctions are not checked.

Implement one small predicate-intersection algebra per argument: intersect exact candidates across all exact constraints, then filter those candidates through every prefix/contains conjunct. Handle prefix-only intersection structurally and fail only when emptiness is proven. Keep this domain implementation instead of introducing a regex automaton crate, and add three-way and pairwise-overlap adversarial tests.

**Fix:** Argument contradiction checking now intersects all exact candidate sets, evaluates every exact candidate against every prefix/contains predicate, and computes prefix-only compatibility by combining comparable prefixes. Tests cover pairwise-overlapping exact sets with empty total intersection, multiple prefix conjuncts, and incompatible prefix-only constraints.

#### READ-007 — `lower_program` still performs three full AST traversals

- **Severity:** High
- **Fix Complexity** Extreme
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:241-332`, `glass-lint-core/src/analysis/scope/mod.rs:88-103`

`ScopeGraph::collect_scoped_program` visits the complete SWC tree once for planning and once for collection; `FactBuilder` then visits it a third time. This is the dominant fixed cost for every uncached file before downstream fact-stream work begins.

Retain only the declaration/hoisting information that truly requires a prepass, then design one source-order semantic traversal that updates live scope state and emits matcher-independent facts. Do not add a fourth adapter traversal or expose half-built scope storage to make the merge easier. Require parity tests for shadowing, reassignment, control flow, imports, and exhaustion plus size/depth benchmarks before changing pass ownership.

#### [x] READ-008 — Flow-state lookups scan the complete live state table

- **Severity:** High
- **Fix Complexity** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:53-131`, `glass-lint-core/src/analysis/flow/projector/state.rs:283-295`, `glass-lint-core/src/analysis/flow/projector/evidence.rs:28-139`

`states_for(object)` and `remove_states_for(object)` iterate the entire `BTreeMap`, even though `FlowStateKey` begins with `object` and supports a bounded range. Configuration and sink handling repeatedly collect those scans into temporary vectors, while `objects()` yields duplicate object IDs once per alias.

Use a `BTreeMap::range` over the object key interval or an object-indexed two-level/dense table, and iterate `object_refs.keys()` for unique live objects. Add direct candidate indexes for property and member requirements so one event touches only relevant `(object, flow, requirement)` entries. Preserve deterministic order at iteration boundaries rather than paying for a globally ordered container in every lookup.

**Fix:** Flow-state lookup and removal now use the object-bounded `BTreeMap::range` interval, and live-object iteration uses the reverse alias index so each object is visited once. A unit test covers duplicate aliases sharing one object while preserving deterministic object order.

#### [x] READ-009 — Cross-call seeding multiplies every argument by every flow

- **Severity:** High
- **Fix Complexity** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/worklist.rs:147-220`

For every root call argument with a qualified target, `seed_from_calls` scans every compiled flow and materializes an explicit unknown-source context for each flow lacking a source candidate. At catalog limits this is call arguments × lifecycle flows before useful propagation starts, and it can consume `MAX_CONTEXTS` on negative alternatives alone.

Represent unknown reachability as a grouped flow set/bitset or generate it only for flows relevant to the target function's downstream operations. Index source candidates by `FlowId` once instead of repeatedly scanning candidate slices inside the all-flows loop. Add scale tests with many unrelated lifecycle rules and many helper calls, verifying both work and certainty.

**Fix:** Cross-call seeding now derives the bounded unknown-flow candidates from the flows that have at least one proven source, rather than iterating every compiled lifecycle flow. Each call site also reuses its source-candidate bucket while checking known and unknown alternatives, avoiding repeated candidate lookups.

### Medium severity

#### READ-010 — Function effects are built eagerly even when no selected rule uses flow

- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:311-332`, `glass-lint-core/src/analysis/facts/mod.rs:483-512`

Every successful lowering constructs `FunctionEffectsBuilder` and records calls, uses, roots, parameters, and returns while building occurrence indexes, even for catalogs containing only ordinary event queries. Combining both products into one fact pass saves a scan but makes the expensive flow product unconditional.

Keep effects matcher-independent but initialize them lazily from the immutable fact stream, for example behind `OnceLock`, when selected `FlowRequirements` first request local or cross-call flow. Cache the completed result in the shared semantic artifact so repeated projections do not rescan. Keep effect-limit failure and status reporting explicit when the lazy phase runs.

#### READ-011 — Resolver `Arc` values allocate and atomically count in the AST hot path

- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:100-129`, `glass-lint-core/src/analysis/resolution/expression.rs:18-117`, `glass-lint-core/src/analysis/resolution/expression.rs:153-224`, `glass-lint-core/src/analysis/resolution/expression.rs:342-386`

The resolver cache stores `Arc<ResolvedValue>`, and literals, unknowns, fresh objects, identifiers, and members commonly allocate a new `Arc` during the third AST traversal. Most consumers immediately copy IDs and clone narrow provenance fields, so atomic ownership is acting as a borrow-checker adapter rather than shared cross-thread state.

Store resolved records in an arena and cache a `ResolvedValueId`, or return owned narrow records for uncached leaves while cache entries use stable indices. Make identity-only calls return `ValueId` without constructing an archived record. Measure allocations per AST node before and after, and retain cycle/error representation as explicit values rather than shared heap objects.

#### READ-012 — The bound flow plan still reinterprets declarations in transfer loops

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:22-150`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:49-93`, `glass-lint-core/src/analysis/flow/projector/evidence.rs:76-198`

`BoundFlowPlan` indexes only `NamePath -> Vec<FlowId>` and keeps parallel per-flow vectors. Source matching then re-walks all sources and repeats `lookup_path`; sink and requirement handling scan declarations, linearly call `contains`, clone summaries, and allocate several `Vec`s for each event.

Bind directly executable source, requirement, and sink candidates keyed by chain/property, including rootedness, argument selectors, flow ID, and declaration index. Replace tiny event-local vectors with streaming loops or the existing `SmallVec` dependency, and replace `CompiledObjectSinkArguments::present_indices`'s boxed iterator with a concrete enum iterator. Keep one canonical bound representation shared by local summaries and cross projection.

#### [x] READ-013 — Flow exhaustion is inferred from full capacity instead of failed work

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:17-90`, `glass-lint-core/src/analysis/flow/projector/mod.rs:697-725`

The outcome is marked exhausted when object, state, or emission counts are exactly equal to their limits, even if no insertion was rejected. The configured `flow_operations` also silently becomes a per-module local allowance multiplied by 16, while cross flow receives the original amount, so the public limit does not describe one comprehensible unit or aggregate bound.

Track explicit rejection/exhaustion flags at each bounded owner and report incomplete only after an attempted operation fails. Define and document whether flow operations are per module, per phase, or per project, then allocate from that budget without an unexplained multiplier. Use the shared `Budget` semantics consistently: reaching capacity is valid; exceeding it is exhaustion.

**Fix:** Local flow now records object and state-limit rejections explicitly, while evidence and operation budgets already expose failed-work state; outcome exhaustion is based on those rejection flags rather than exact capacity. Local operations use the configured per-module allowance without the unexplained 16× multiplier, and cross propagation retains its project-phase budget.

#### READ-014 — “Normalized” lifecycle IR retains public authoring types

- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:199-218`, `glass-lint-core/src/api/compiler/object_flow.rs:53-149`

`NormalizedLifecycle` stores `LifecycleCondition` and `LifecycleCompletion` from the public API, and physical lowering reinterprets their authoring enums. Sources also flatten `CanonicalArgumentConstraints` back into `Vec<ArgumentConstraint>`, recreating the representation normalization just removed.

Define normalized lifecycle source, requirement, sink, and completion variants that contain canonical paths and grouped constraints. Compile the physical flow directly from those variants, then delete `to_flat_vec` and the `from_matcher` adapters. Make this a breaking single-path migration; do not preserve a reverse compatibility conversion.

#### READ-015 — Dense compiler slots survive after their consumers disappeared

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:41-47`, `glass-lint-core/src/api/compiler/normalize.rs:38-68`, `glass-lint-core/src/api/compiler/normalize.rs:126-160`, `glass-lint-core/src/api/compiler/normalize.rs:240-310`

`NormalizedEmission::primary_slot` is alpha-renumbered, collected, remapped, and validated, but physical planning consumes only evidence kind and symbol. The same normalizer also linearizes already-canonical argument groups, clones and sorts them, then rebuilds the identical canonical type as a future-proofing adapter.

Delete `primary_slot` and the alpha-renumbering machinery once validation proves the authored primary binding before normalization, retaining only object slots that physical correlation actually uses. Make canonical argument construction the sole validating constructor and remove the re-canonicalization pass. If future multi-event operators require slots, reintroduce a typed correlation table with an executable consumer rather than carrying dormant compiler scaffolding.

#### READ-016 — `NormalizedEvent` permits invalid identity/subject combinations

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/api/compiler/normalized.rs:157-197`, `glass-lint-core/src/api/compiler/physical.rs:255-317`

An event stores `identity: Option<IdentitySpec>` separately from `NormalizedSubject`, so `Direct + None`, `Returned + Some`, and incompatible event/subject combinations are representable. Physical planning relies on `expect` and substitutes empty member paths for impossible variants, spreading one cross-field invariant across normalization, validation, and planning.

Replace the pair with a sum type such as `Direct { identity }`, `Returned { producer, object_slot }`, and `Instance { constructor, object_slot }`, with event-compatible constructors. Let physical planning exhaustively match valid variants without `expect` or empty sentinels. Keep validation for authored errors, not for states the internal type can prevent.

#### [x] READ-017 — Lifecycle argument builders enforce the wrong dimension

- **Severity:** Medium
- **Fix Complexity** Low
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:40-59`, `glass-lint-core/src/api/rule/query/lifecycle.rs:129-152`, `glass-lint-core/src/api/rule/query/mod.rs:169-185`

`LifecycleSource::with_arg` and `LifecycleEventBuilder::arg` compare total constraint count with `MAX_PREDICATES_PER_ARGUMENT`. They therefore reject 33 distinct argument groups although ordinary `EventQuery` allows 64, and their reported index/count do not necessarily describe the group that exceeded its bound.

Move one bounded `ArgumentConstraintsBuilder` beside the value API and use it from event, lifecycle-source, and lifecycle-event construction. Enforce group count and per-index predicate count incrementally, then freeze directly into canonical groups. Add exact-boundary tests for many groups, many predicates on one group, duplicates, and mixed indices.

**Fix:** A shared incremental `ArgumentConstraintsBuilder` now enforces the 64-group and 32-predicate-per-index limits for ordinary events, lifecycle sources, and lifecycle events, then freezes constraints in canonical order. Boundary tests cover maximum groups and per-index predicate counts.

#### READ-018 — Public query strings are validated repeatedly but not normalized consistently

- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Newtype
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:162-264`, `glass-lint-core/src/api/rule/query/mod.rs:267-645`, `glass-lint-core/src/api/rule/query/value.rs:55-110`, `glass-lint-core/src/api/rule/query/value.rs:136-178`, `glass-lint-core/src/api/rule/query/value.rs:205-232`

The 1,600-line query module repeats empty/module/chain checks across constructors, often checks `trim().is_empty()` but retains surrounding whitespace, and has inconsistent holes: `call_package` accepts an empty export, `equals` accepts an empty value, and object-property matching accepts an empty property. The normalizer claims to trim names but only rebuilds constraints, so authoring and compiler guarantees diverge.

Introduce private validated `IdentityName`, `ModuleName`, `MemberChain`, `PropertyName`, and non-empty static-value constructors, then make every public combinator delegate to them. Decide canonical whitespace semantics once at construction and store only the canonical value. Keep `SymbolPath` as the established path parser; do not add a parser crate for this small grammar.

#### [x] READ-019 — `QueryDecl::any` silently chooses the first branch’s evidence symbol

- **Severity:** Medium
- **Fix Complexity** Low
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:970-1003`

Alternative construction verifies primary variable and `MatchKind` but not the evidence symbol, then stores the first branch's emission. The documented `fetch | navigate` example therefore labels every branch with `fetch`, making evidence dependent on declaration order.

Require identical emission descriptors across branches, or add an explicit `any_with_evidence`/named-alternative API and require the caller to choose the aggregate symbol. Do not silently derive an aggregate label from the first branch. Add permutation tests proving branch order cannot change evidence metadata.

**Fix:** `QueryDecl::any` now rejects alternatives with different evidence symbols, while `any_with_evidence` lets callers provide an explicit aggregate label after the branch kind and primary variable are validated. Existing alternative callers were migrated to the explicit API so branch order no longer selects metadata implicitly.

#### [x] READ-020 — Project execution maintains a bespoke thread pool per call

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/session/execution.rs:167-248`

Each `analyze_sources` call collects all jobs, creates two channels, wraps the receiver in a contended mutex, spawns fresh OS threads, and manually joins them. An empty job set still computes one worker and spawns a thread, and the implementation duplicates mature scheduling, panic, and worker-reuse behavior.

Use a bounded Rayon pool or scoped parallel iterator, then merge results deterministically in the existing canonical maps. Rayon is the established crate justified here; keep the `LocalJobExecutor` seam for deterministic tests. Add an immediate zero-job return regardless of the eventual executor.

**Fix:** Local lowering now runs through a bounded Rayon pool in bounded batches, preserving the existing executor seam and deterministic release path. Empty job sets return immediately, and observer accounting remains bounded by the same outstanding-job limit.

#### READ-021 — Every source is lexed twice and regex context is partly handwritten

- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/parse.rs:108-196`, `glass-lint-core/src/parse.rs:232-323`

The syntax-depth prepass runs the SWC lexer across the full source, manually reconstructs regex-vs-division context and regex ends, and then SWC lexes the source again during parsing. This adds a linear pass to every file and keeps a small handwritten JavaScript lexical model on an adversarial boundary.

Move depth accounting into a token wrapper consumed by the actual SWC parser, or add/use an upstream SWC depth hook so one contextual lexer drives both checks and parsing. Continue relying on SWC/its `stacker` support rather than adding a regex crate, which cannot decide JavaScript lexical goal. Preserve the pre-allocation safety property while eliminating duplicate tokenization.

#### READ-022 — Constrained matchers rebuild execution scaffolding per module

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:24-168`

Each projection filters physical roots into a new constrained vector, builds a parallel `PreparedClausePaths` vector, and allocates fallback and per-root occurrence vectors. When an index lookup is unavailable, execution becomes facts × fallback roots even though event kind and most identity dimensions could still narrow candidates.

Split catalog-stable constrained descriptors in `ProjectionPlan` and bind only module-local `NameId` paths per module. Bucket fallback roots by event kind and any available identity prefix before scanning facts, and stream matched occurrences into owned evidence buffers. Keep the linear fallback only for semantic cases overlays truly cannot index.

#### READ-023 — Flow completion state uses ordered maps for a bounded 64-slot domain

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:148-212`, `glass-lint-core/src/analysis/model/flow.rs:214-300`

`RequirementSet` is an `Arc<BTreeMap<usize, BTreeSet<K>>>` even though lifecycle requirements and sinks are capped at 64. Hot-path cloning, hashing, readiness, mutation logging, and fingerprinting therefore traverse tree nodes and trigger copy-on-write maps for what is fundamentally a small indexed domain.

Use a completion `u64` mask plus compact per-index trace evidence, using the existing `SmallVec` where multiple facts must be retained. If the 64 cap is intentionally removed later, `fixedbitset` is the appropriate established crate; under the present invariant a domain newtype over `u64` is simpler. Keep evidence order explicit and deterministic rather than deriving it from general-purpose map order.

#### READ-024 — Hot-path regressions have no repeatable performance gate - OUT OF SCOPE FOR NOW

### Low severity

#### [x] READ-025 — Lifecycle APIs retain compatibility-shaped always-successful `Result`s

- **Severity:** Low
- **Fix Complexity** Low
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:124-180`, `glass-lint-core/src/api/rule/query/lifecycle.rs:286-296`

`LifecycleEventBuilder::build` is explicitly allowed past `clippy::unnecessary_wraps`, and `LifecycleCompletion::configuration` also returns `Result` despite having no failure path. These signatures propagate extra `unwrap`/adapter plumbing and preserve the shape of an earlier builder API.

Return the value directly and let fallible inputs remain accepted through focused `IntoLifecycleEvent`/completion adapters where useful. Update all callers in the same breaking change, as repository policy permits. Do not add a compatibility wrapper solely to retain the old return type.

Fix: `LifecycleEventBuilder::build` and `LifecycleCompletion::configuration` now return their infallible values directly. `IntoLifecycleEvent` and the new `IntoLifecycleCompletion` adapter preserve fallible authoring inputs at the query-builder boundary, so callers no longer unwrap an always-successful result.

## Systemic Themes

- Bounds are often checked at the output edge rather than charged where CPU and allocation work occurs. The evidence cap, cross-flow Cartesian seeding, per-module 16× local allowance, and full-state scans are manifestations of the same ownership problem.
- Several “optimized” data structures expose checkpoints but retain whole-state work at joins. Complexity claims should describe the complete operation, including root-path construction, snapshots, candidate scans, and canonical hashing.
- The compiler currently has declaration, normalized, and physical layers, but lifecycle authoring types and dormant variable slots cross those boundaries. Each layer should contain only validated state consumed by the next layer.
- Public query construction needs semantic newtypes and one constraints builder. Repeated string checks and hand-built `QueryExpr` atom vectors are producing inconsistent limits and validation holes.
- Determinism does not require `BTreeMap`/`BTreeSet` in every hot mutation. Dense IDs, hash/delta structures, and compact bitsets can sort only at freeze/report boundaries.
- The right external-crate opportunities are Rayon for execution and, only if the lifecycle cap grows, `fixedbitset`; existing SWC, `petgraph`, `smallvec`, and `smol_str` should be used more completely before adding alternatives.

## Open Questions

None remain unresolved. The audit records these decisions:

- Treat hash-only flow-state coalescing as incorrect, even if collisions are unlikely; strict path-local identity requires equality.
- Replace the normalized-lifecycle/public-type adapter with one canonical compiler IR and update all callers; do not keep a compatibility path.
- Keep `petgraph` for SCC work, the fixed-size FIFO artifact cache, and the test-only limited reference evaluator. None should be replaced merely for uniformity.
- Use Rayon for project job scheduling. Do not add `lru`, regex, glob, or persistent-map crates for the current problems.
- Prefer domain delta logs and compact masks/`SmallVec` buffers for scope and flow state. Benchmark generic persistent collections only as alternatives, not as the default decision.
- Preserve the three-pass lowering semantics until a combined frontend can prove parity; do not trade correctness for a superficial pass-count reduction.

## Coverage

Reviewed all production modules under `glass-lint-core/src`, all core integration-test areas, crate/root architecture and testing guidance, dependency declarations, and recent diffs affecting compiler, lowering, scope, facts, local/cross flow, project execution, and reports. The most detailed inspection covered `analysis/lowering`, `analysis/scope`, `analysis/resolution`, `analysis/facts`, `analysis/flow`, `analysis/matching`, `api/rule/query`, `api/compiler`, and `project/session`.

Recent history was compared across the last twelve core-changing commits, including the new normalized/physical compiler split and the flow checkpoint/coalescing changes. `cargo clippy -p glass-lint-core --all-targets --all-features -- -D warnings` completed successfully. This was a static design/readability audit rather than a profiler run; the absence of a repeatable benchmark suite is itself recorded as READ-024.
