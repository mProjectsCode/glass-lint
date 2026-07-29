# `glass-lint-core` Readability and Performance Audit

## Summary

This read-only audit covers all production and test source under `glass-lint-core`, with extra attention to the recent query API/compiler and flow-matcher work and to hot paths beginning at `lower_program`. The crate's `src` tree contains 51,776 lines of Rust: 30,408 in `analysis`, 11,551 in `api`, 5,485 in `project`, 1,316 in `lint`, and 652 in terminal report rendering.

I found 29 actionable issues: 11 High, 16 Medium, and 2 Low severity. The most urgent correctness defects are an exact rule selector matching longer rule IDs and helper-sink propagation losing transitive sinks according to function order. The largest static performance risks are repeated whole-AST passes, deep cloning of scope environments, quadratic flow-state coalescing, whole-state cloning on each flow edit, copying loop facts on every fixed-point iteration, a mutex on every terminal value lookup, and an unbounded hand-built worker pool.

**Completed (11 of 29):** READ-001, READ-002, READ-005, READ-009, READ-010, READ-014, READ-016, READ-026, READ-028, READ-029, READ-013. Remaining: 6 High, 12 Medium.

The new query compiler has a sensible declaration → normalized IR → physical IR direction, but it still contains a reverse lifecycle adapter, repeated tree walkers and canonicalizers, a duplicate convenience API, and a partial “reference” evaluator that does not cover the newly important lifecycle path. The flow implementation is bounded in many dimensions, but several bounds cap retained output rather than the CPU and allocation work used to reach it.

`cargo test -p glass-lint-core --all-features` passed: 682 unit tests plus all integration and doc-test binaries. Five tests remain ignored, including the three limit/certainty tests discussed in READ-027. `cargo clippy -p glass-lint-core --all-targets --all-features -- -W clippy::pedantic` also completed successfully; its 272 library warnings and 283 test-build warnings were predominantly documentation and `must_use` suggestions and are not repeated as findings.

## Architecture Summary

The current semantic pipeline is:

1. copy source into SWC, pre-scan syntax depth, and parse;
2. plan scopes and seed names with one AST traversal;
3. collect source-order scope and assignment semantics with a second traversal;
4. build matcher-independent facts with a third traversal;
5. freeze names/values, derive occurrence indexes and function effects;
6. link modules, project compiled matchers, run local/cross-call/cross-file flow, and assemble reports.

The core abstractions are generally moving in the right direction: facts are matcher-independent, query declarations compile once, stable ID newtypes are common, and project stages consume `self`. The main architectural weakness is that phase ownership is not consistently reflected in types. That produces immutable artifacts containing `Mutex` or `RefCell`, `Arc` used as a borrow-checker workaround, unsafe edit guards, and repeated conversions between already-validated representations.

The crate is large, but most semantic modules share private IDs, tables, and fail-closed invariants. Splitting that semantic graph into crates now would expose implementation storage and create more adapter APIs. Presentation and telemetry are different: they have clean outward-facing boundaries and only higher-level consumers, so they should leave core now. The detailed decision is recorded under READ-024 and Open Questions.

## Findings

### Rule selection

#### READ-001 — Exact rule selectors match longer rule IDs [Done]

- **Severity:** High
- **Fix Complexity** Low
- **Category:** API
- **Location:** `glass-lint-core/src/lint/selection.rs:75-142`

`RuleSelector::matches` treats the first literal specially and only calls `starts_with`. For a selector without `*`, that first literal is also the final literal, but the end-anchor branch is never reached. An exact selector such as `js:foo` therefore matches `js:foobar`, so validation can accept and enable a different rule than the user named.

Add a no-wildcard fast path using `id == self.raw`, then add exact-prefix, leading/trailing/multiple-wildcard, empty-match, and Unicode table tests. Keep the deliberately tiny in-house `*` grammar; adding `globset` for this single-selector language is not justified unless the language expands.

**Fix:** Added `id == self.raw` early return before the wildcard loop. Added 14 focused unit tests covering exact match, exact rejection of longer/shorter/different IDs, trailing/leading/both-sides wildcards, and the `*:*` catch-all pattern.

### Flow summaries

#### READ-002 — The helper-summary delta cursor loses transitive sinks [Done]

- **Severity:** High
- **Fix Complexity** Medium
- **Category:** Testing
- **Location:** `glass-lint-core/src/analysis/flow/summary/summaries.rs:142-223`, `glass-lint-core/src/analysis/flow/summary/summaries.rs:226-287`

`propagate_sinks` processes a round, advances every changed summary's global `sinks_offset` to its current length, and only then schedules reverse callers. If a caller ran before its callee in that round, it is retried next round, but the callee's cursor has already advanced, so `propagate_call_sinks` sees no new sinks. Multi-hop propagation is consequently dependent on `FunctionId`/source order.

Remove the global per-summary cursor and let the existing sink set deduplicate, or keep a version/cursor per call-graph edge. Add three-or-more-hop tests in both declaration orders, a diamond, recursion with a newly discovered sink, and a permutation property asserting source-order independence.

**Fix:** Removed the per-summary `sinks_offset` cursor entirely. `propagate_call_sinks` now iterates all sinks and the `SinkSet::push_unique` (based on `IndexSet`) deduplication prevents redundant work. The cursor was being advanced on callers instead of targets, causing transitive sinks to be silently skipped in multi-hop call graphs.

### Parsing and local lowering

#### READ-003 — Any backtick selects a second handwritten JavaScript lexer

- **Severity:** High
- **Fix Complexity** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/parse.rs:113-177`, `glass-lint-core/src/parse.rs:252-359`, `glass-lint-core/src/parse.rs:362-557`, `glass-lint-core/src/parse.rs:631-651`

Ordinary input is depth-scanned with SWC tokens before being parsed by SWC, but any source byte equal to a backtick bypasses that path. `template_syntax_depth` then reimplements quotes, comments, regular-expression classes, regex-vs-division context, nested templates, delimiters, and member chains. A backtick in a comment or string is enough to select it. This is a duplicate language frontend on an adversarial pre-parse path, and it cannot stay aligned with ECMAScript/SWC.

Delete the byte lexer. Obtain template-expression boundaries from the same SWC frontend that parses the file, or add a bounded token/context hook upstream and make parsing itself return the depth failure. SWC is already the high-quality crate to use here; a regex crate would not solve lexical context.

#### READ-004 — Each cache miss walks the whole AST at least three times

- **Severity:** High
- **Fix Complexity** Extreme
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:119-135`, `glass-lint-core/src/analysis/lowering/mod.rs:231-300`, `glass-lint-core/src/analysis/scope/mod.rs:88-103`, `glass-lint-core/src/analysis/scope/build/plan.rs:176-215`

`ScopeGraph::collect_scoped_program` performs a planning traversal and a collection traversal, after which `FactBuilder` performs a third complete SWC traversal. The planner visits every identifier, member, and property to seed names, not only declarations needed for hoisting. The frozen fact stream is then scanned again to construct `SemanticFacts` indexes and again to collect `FunctionEffects`.

Retain a narrow hoisting/scope-shape prepass, but stop using it as a general name census. Design one semantic frontend traversal that collects source-order scope state and emits facts against the frozen declaration plan, then build occurrence indexes and effects in one fact-stream pass. Because this is the central semantic invariant, require lowering benchmarks and adversarial semantic parity tests before merging phases.

#### READ-005 — Exhausting a semantic budget does not stop AST traversal [Done]

- **Severity:** High
- **Fix Complexity** High
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/scope/build/plan.rs:176-199`, `glass-lint-core/src/analysis/facts/mod.rs:166-204`, `glass-lint-core/src/analysis/lowering/mod.rs:238-300`

Scope visitors continue walking after `try_charge` fails. `FactBuilder::emit` returns once the shared budget is exhausted, but SWC's visitor still descends through the rest of the program, and lowering proceeds to export-origin and effects phases. The configured limit therefore bounds recorded operations but not the CPU spent visiting a hostile oversized AST.

Make cancellation a phase-level result. Use an explicitly stoppable walker or a visitor gate that prevents child descent after exhaustion, and do not start later derived phases when their required input is already invalid. Tests should count visited nodes below/at/above each limit, not only emitted facts and status codes.

**Fix:** Added `is_budget_exhausted` to `ScopePass` trait and guarded every `ScopeTraversal` visitor method's child descent behind it. Added the same early-return guard to every `FactBuilder::visit_*` method. In `lower_program`, export-origin processing and effects collection are now skipped when the budget is exhausted or the stream is structurally invalid. Added `tiny_semantic_budget_stops_traversal` and `large_semantic_budget_produces_complete_artifact` tests verifying that traversal stops, effects/export origins are empty under exhaustion, and the analysis completes without panic at every limit.

#### READ-006 — Scope branch checkpoints deep-clone all assignment state

- **Severity:** High
- **Fix Complexity** High
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/scope/build/assignments.rs:127-170`, `glass-lint-core/src/analysis/scope/build/history.rs:12-145`, `glass-lint-core/tests/scope_precision.rs:308-363`

Every conditional, loop, switch, and `try` checkpoint clones the nested `HashMap<ScopeId, HashMap<NameId, ProvenanceAlternatives>>` plus the write set. Joins rebuild maps for every active scope/name and deduplicate provenance with `Vec::contains`. There is no bound on the number of provenance alternatives; the intended alternative-limit tests are still ignored.

Reuse the mutation-log/checkpoint approach already present in facts and flow, or use an immutable/persistent map only after benchmarking it against a domain-specific delta log. Store touched bindings per branch and join only those deltas. Introduce an explicit alternative budget whose exhaustion produces unknown/possible, then enable the ignored certainty tests.

### Local flow projection

#### READ-007 — Flow path coalescing is quadratic in alternatives and copies every state

- **Severity:** High
- **Fix Complexity** High
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:549-587`, `glass-lint-core/src/analysis/flow/projector/state.rs:170-215`

For every path at every join, `join_paths` restores the path, clones the complete alias and flow-state maps, and linearly compares that snapshot with all prior snapshots. The cost is O(alternatives² × live state), with thousands of alternatives allowed. The operation budget eventually stops comparisons, but only after repeatedly allocating and copying the state being compared.

Give each mutation-log state a canonical incremental fingerprint or interned state ID and use a hash set for membership with full equality only on hash collision. Normalize object IDs once when a semantic state is frozen/interned, not for each comparison. Deterministic report order does not require a `BTreeSet` in this internal membership test.

#### READ-008 — Every flow-state edit deep-copies COW maps and uses an unsafe guard

- **Severity:** High
- **Fix Complexity** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:138-176`, `glass-lint-core/src/analysis/flow/projector/state.rs:269-300`, `glass-lint-core/src/analysis/model/flow.rs:143-205`

`state_mut` clones the entire old `FlowState`, obtains a raw pointer into the map, and returns a `StateEdit` that dereferences it unsafely. Because `RequirementSet` is `Arc<BTreeMap<...>>`, cloning the old state increments the `Arc`; the first edit then makes `Arc::make_mut` copy the whole requirement or sink map. `Drop` clones the new state again to record the inverse delta.

Put fine-grained mutation methods on `FlowStateTable` and log typed deltas such as requirement inserted/removed or sink inserted/removed. That removes the raw pointer, full-state snapshots, and accidental COW deep copies. The table—not a deref guard—owns both the state and its rollback invariant.

#### READ-009 — Loop fixed-point replay clones the entire fact slice each iteration [Done]

- **Severity:** High
- **Fix Complexity** Medium
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:324-350`

`replay_loop_body` calls `.to_vec()` on the loop's `SemanticFact` slice for every replay iteration so it can mutate `self` while iterating its stream. Fact payloads contain paths, arguments, and provenance, so a large loop repeatedly allocates and clones its whole semantic body.

Split immutable projection context (`FactStream`, names, plan, summaries) from mutable execution state, or replay `FactId` indices through a method that borrows each fact only for the transfer call. No semantic fact should be copied merely to satisfy a broad `&mut self` receiver.

**Fix:** Replaced `self.stream.facts()[start..end].to_vec()` with a zero-copy indexed iteration over a local immutable stream reference. The stream borrow is independent of the projector's mutable state, so each fact can be transferred without cloning semantic facts or using raw pointers.

### Value model

#### READ-010 — Immutable value resolution locks a mutex on every lookup [Done]

- **Severity:** High
- **Fix Complexity** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/model/value.rs:56-128`, `glass-lint-core/src/analysis/model/value.rs:163-235`

The frozen `ValueTable` retains `Mutex<Vec<Option<ValueId>>>`, and every `resolve`/`resolve_id` takes that mutex. These calls are pervasive in fact indexing, matching, and flow. Yet `intern` already enforces that a binding target has a lower ID, so the target's terminal ID is known when the binding is inserted.

Compute and store the terminal ID eagerly at insertion/freeze and make resolution a direct immutable lookup. This removes locking, path-compression complexity, poison handling, and the manual `Clone` implementation. If a separate resolved arena is desirable, use dense IDs rather than `Arc<ResolvedValue>`/mutexes as ownership adapters.

**Fix:** Replaced `Mutex<Vec<Option<ValueId>>>` with a plain `Vec<ValueId>`. `intern` now eagerly computes the terminal ID for bindings from the already-known target terminal and stores it directly. Removed `MAX_RESOLVE_HOPS`, the chain-walking path-compression loop, the `SmallVec` allocation, the manual `Clone` impl (now derived), and all mutex locking/poison handling. `resolve_terminal` is now a single `self.terminal_cache.get(idx).copied()` lookup.

### Project execution

#### READ-011 — The local executor can create arbitrarily many threads

- **Severity:** High
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/session/execution.rs:167-245`, `glass-lint-core/src/project/session/execution.rs:300-305`

Each analysis call creates `worker_limit` scoped OS threads, even for an empty or one-file job stream. `normalize_worker_limit` only changes zero to one; it does not cap the request by job count or `available_parallelism`. A caller-supplied large value can exhaust threads and creates an equally large channel bound. Workers also serialize `recv` through a mutex around the standard MPSC receiver.

Use a bounded Rayon pool (or accept an executor from the host) and cap active workers to `min(requested, uncached_jobs, available_parallelism)` unless the API explicitly documents oversubscription. Collect indexed results and release them through the existing deterministic assembly boundary. Rayon is a better-established implementation than maintaining a custom pool/channel protocol here.

### Query compiler: lifecycle physical planning

#### READ-012 — Lifecycle compilation reverses normalized IR back into the public API

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/physical.rs:377-402`, `glass-lint-core/src/api/compiler/object_flow.rs:15-79`

`plan_lifecycle` clones a `NormalizedLifecycle` into public `EventQuery` values, invents the name `"lifecycle"`, reconstructs a validated `LifecycleQuery`, calls `.expect`, and then has `CompiledObjectFlow` interpret that query again. This is a legacy reverse adapter inside the new forward compiler.

Compile `NormalizedLifecycle` directly into the physical flow IR. The normalized representation has already established source slots, canonical constraints, condition, and completion; rebuilding an authoring type adds allocations, a panic assertion, and a second interpretation of invariants.

### Compiled flow representation and fixed points

#### READ-013 — Compiled object flow is a boolean state machine with per-event allocations [Done]

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/api/compiler/object_flow.rs:15-79`, `glass-lint-core/src/api/compiler/object_flow.rs:140-157`, `glass-lint-core/src/analysis/flow/projector/evidence.rs:35-129`

`all_requirements_required`, `all_sinks_required`, and `emit_on_requirements` encode mutually dependent modes as three booleans. `evidence_symbol` clones a `String`, and `present_indices` allocates a `Vec` for each sink call. Flow event handling also allocates temporary object, key, flow-ID, pair, and matching-sink vectors to work around mutable-borrow boundaries.

Replace the booleans with exhaustive enums such as `RequirementMode` and `CompletionMode`; store the symbol as `SmolStr` or return `&str`; return an iterator for sink indices. After READ-008, reuse bounded scratch buffers or indexed lookups so ordinary event transfer does not allocate several short-lived vectors.

**Fix:** Replaced the three booleans (`all_requirements_required`, `all_sinks_required`, `emit_on_requirements`) with exhaustive `RequirementMode` (`AllRequired`/`AnyRequired`) and `CompletionMode` (`Configuration`/`AnySink`/`AllSinks`) enums. Changed `symbol` from `String` to `SmolStr`; `evidence_symbol()` returns `&SmolStr` instead of cloning. `present_indices` now returns `Box<dyn Iterator<Item = usize>>` instead of allocating a `Vec`.

#### READ-014 — Fixed-point completeness is limited by arbitrary 64-round caps [Done]

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary/summaries.rs:158-223`, `glass-lint-core/src/analysis/flow/cross/mod.rs:40`, `glass-lint-core/src/analysis/flow/cross/state.rs:11-29`

Helper-sink propagation and cross-source refinement stop after 64 rounds even though both are monotone worklists with separate size/operation bounds. A valid chain deeper than 64 becomes incomplete solely because of graph depth, and the cap obscures whether work, state, or depth was actually exhausted.

Run worklists until stable or until the typed operation/state budget is exhausted. Charge each edge/candidate transfer and report that precise bound. A semantic depth restriction, if desired, should be an explicit public analysis limit with adversarial tests rather than a private magic number.

**Fix:** Removed `MAX_SUMMARY_ROUNDS` (64) from summary propagation — the loop now runs until the worklist is empty or the `Budget`/sink-capacity limits are exhausted. Replaced `MAX_SOURCE_REFINEMENT_ROUNDS` (64) and the round-based `SourceBudget` with a per-transfer `Budget` that charges each candidate insertion. The source worklist now also runs until stable (empty pending) or budget exhausted. `FlowLimits` operations budget is passed from the cross-module collector into source propagation.

#### READ-015 — Local and cross flow independently pre-resolve the same paths

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:22-103`, `glass-lint-core/src/analysis/flow/cross/graph.rs:53-84`

`BoundFlowPlan` and `FlowPathPlan` both walk every compiled requirement and sink and convert the same `SymbolPath` values through the same module `NameTable`. One serves local projection and the other cross-flow contexts, so the recent flow split created duplicate planning logic and storage.

Build one module-bound flow plan containing source, requirement, and sink paths and share it across local, summary, and cross-module stages. This also gives one place to choose hot hash indexes and to enforce deterministic iteration at freeze time.

### Query compiler: validation, normalization, and tests

#### READ-016 — Branch compatibility ignores the requested evidence variable [Done]

- **Severity:** Medium
- **Fix Complexity** Low
- **Category:** Testing
- **Location:** `glass-lint-core/src/api/compiler/normalize.rs:330-379`, `glass-lint-core/src/api/compiler/validate/pass1_3.rs:228-376`

`check_branch_evidence_compatibility` passes the primary variable to `branch_var_type`, but that function names it `_var` and classifies only the root event. Its diagnostics use placeholder strings `"some"` and `"other"`. The earlier type-checking pass already implements variable-aware compatibility, so this duplicate normalizer check can reject or describe the wrong thing as the expression language grows.

Remove the duplicate and have normalization consume validated typed metadata, or make a single shared variable-type query authoritative. Diagnostics must carry the actual `VarType` names; placeholder text should not survive an internal compiler error path.

**Fix:** Replaced `if first_type != other && first_type.is_some() && other.is_some()` with `if let (Some(a), Some(b)) = (first_type, other) && a != b`, using the actual type values instead of hard-coded `"some"`/`"other"` placeholders in the error diagnostic.

#### READ-017 — Query-tree walking and constraint canonicalization are reimplemented repeatedly

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/expression.rs:90-126`, `glass-lint-core/src/api/compiler/validate/error.rs:258-318`, `glass-lint-core/src/api/compiler/normalize_all.rs:97-146`, `glass-lint-core/src/api/compiler/normalize_all.rs:319-354`, `glass-lint-core/src/api/compiler/normalize.rs:99-108`, `glass-lint-core/src/api/compiler/physical.rs:63-96`

Variable collection/reference checks have at least five recursive match trees. Argument constraints are sorted/deduplicated during authoring/normalization, copied and canonicalized again after normalization, then `compile_argument_constraints` still performs `contains` checks and repeatedly converts a boxed slice back to `Vec` for each predicate in a group.

Add one internal `QueryExpr` walker/fold with explicit bound/reference callbacks and introduce invariant-bearing canonical constraint/group types. Validate and canonicalize once at the public/compiler trust boundary; physical lowering should consume those newtypes without rechecking or reallocating.

#### READ-018 — Ten compiler passes repeatedly traverse a bounded, already-validated tree

- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/api/compiler/validate/pass4_10.rs:448-468`, `glass-lint-core/src/api/compiler/mod.rs:225-245`, `glass-lint-core/src/api/compiler/normalize.rs:23-67`

Every declaration runs ten validation passes, normalization, post-normalization validation (including recomputing requirements), planning, and whole-plan validation. Query limits make this cold compared with source analysis, but the number of independent semantic passes is why checks and walkers have drifted. Numerous `allow(dead_code)` and unused re-exports in the recently split compiler show that old phase seams remain.

Define one trusted transition from authoring AST to typed normalized IR and combine related validation while types are already in hand. Keep normalized/physical invariant validation in debug and test builds unless it validates untrusted deserialization. Delete dead explain/accessor surfaces after the new compiler API stabilizes.

#### READ-019 — `QueryDecl` duplicates the complete `EventQuery` constructor surface

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:130-275`, `glass-lint-core/src/api/rule/query/mod.rs:833-930`

The 1,787-line query module exposes the same large constructor matrix on `EventQuery` and again as forwarding conveniences on `QueryDecl`. Some wrappers are simple delegation while others repeat validation and construction, so every new event form expands two public APIs, docs, and tests.

With breaking changes allowed, keep the validated constructors on the type that owns the event and one `IntoQueryDecl`/`into_query` conversion. Reserve `QueryDecl` constructors for genuine expression composition (`any`, `all`, lifecycle), not aliases for every event constructor.

#### READ-020 — The compiler “reference” evaluator omits the new flow semantics

- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Testing
- **Location:** `glass-lint-core/src/api/compiler/reference.rs:105-145`, `glass-lint-core/src/api/compiler/reference.rs:245-262`, `glass-lint-core/src/api/compiler/reference.rs:330-447`

Both logical and physical reference evaluation return an empty result for lifecycle roots. Returned/instance subject evaluators ignore the member path, and identity comparison supports only a subset of variants. The oracle is test-only, but its name suggests broader differential coverage than it provides precisely where recent compiler and flow changes are riskiest.

Either extend it into an independent lifecycle/correlation interpreter and generate bounded query/row cases, or rename it to the subset it actually covers and make unsupported capabilities explicit test failures rather than empty matches. Add differential tests for every physical root and identity variant before relying on it as a compiler oracle.

### Lowering status and budget observability

#### READ-021 — Facts exhaustion conflates unrelated limits and invalid states

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:151-172`, `glass-lint-core/src/analysis/lowering/mod.rs:238-266`

One `semantic_operations` number is passed both to `SemanticBudget` and as `FactBuilder`'s fact limit. `check_facts_budget` then reports semantic-budget, fact-count, path-capacity, value-arena, and structural-invalidity failures as `AnalysisComponent::Facts` with the semantic-operation limit and budget-used count. The diagnostic cannot identify which resource actually caused the incomplete result.

Use separate semantic-step, fact-count, path, name, and value limits or a typed exhaustion enum carrying the owning capacity and observed value. Structural mismatch should remain its own reason. Accurate telemetry is necessary before optimizing READ-004 through READ-010.

### Project reporting, session ownership, and crate boundaries

#### READ-022 — Report assembly repeats range work under a long-lived arena lock

- **Severity:** Medium
- **Fix Complexity** High
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/project/model.rs:195-210`, `glass-lint-core/src/analysis/project/projection.rs:69-99`, `glass-lint-core/src/lint/report.rs:178-292`

The finalized project owns `Mutex<TraceArena>`. Projection locks it for the whole project pass; report assembly locks it again while grouping findings, converting spans, resolving traces, allocating messages, and deduplicating traces. Occurrence ranges are converted once for grouping and then recomputed by filtering every evidence occurrence for each retained group. Trace dedup uses `Vec::contains`.

Make the arena a mutable projection-stage owner and freeze/move it into `ProjectionOutcome`; report assembly should borrow an immutable arena without locking. Build converted occurrence records once, index them by retained range, and deduplicate traces with `IndexSet`/`HashSet` while retaining deterministic insertion order.

#### READ-023 — Project session state retains unused and duplicated ownership

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/session/mod.rs:40-87`, `glass-lint-core/src/project/session/mod.rs:140-172`, `glass-lint-core/src/project/input.rs:30-37`, `glass-lint-core/src/project/session/mod.rs:432-475`

`ProjectCollection` stores the artifact cache both inside `SessionState` and as a cloned direct field. It also accepts and stores `_root`, but `normalize_root` only rejects an empty path and no later core operation reads the root. At resolution, `source_map` is cloned wholesale into `ResolvedLinkInput` because reporting retains another owner.

Remove the duplicate cache field and the meaningless core root parameter; the `glass-lint-project` crate already owns filesystem/project boundaries. Split source metadata needed by linking from source text/context needed by reports, or make the link builder consume and return the map, so a stage transition does not clone the complete project table.

#### READ-024 — Terminal presentation and subscriber configuration belong outside core

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/report/render.rs:1-25`, `glass-lint-core/src/telemetry.rs:1-93`, `glass-lint-core/Cargo.toml:7-31`

Terminal rendering is consumed by `glass-lint-cli`; telemetry subscriber setup is consumed by the CLI crates. Nevertheless core depends on `console` and optional `tracing-subscriber`, and its telemetry filter hard-codes higher-level crate targets (`glass_lint_project`, CLI, and harness). That is a dependency inversion and the cleanest available crate split.

Move terminal rendering to a small `glass-lint-output` crate. Move subscriber/options setup to CLI support; core should only emit `tracing` events. Do not split lowering/scope/facts/flow or the query compiler into crates yet: their private IDs and stage invariants are still too coupled, and a crate boundary would force internal execution IR public. Reassess a `glass-lint-query` crate only after READ-012 and READ-017 establish an opaque stable compiler boundary and cargo-timing data shows a build benefit.

### Public rule and environment APIs

#### READ-025 — Core requires provider category policy and then discards it

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/rule/mod.rs:28-45`, `glass-lint-core/src/api/rule/mod.rs:178-208`, `glass-lint-core/src/api/compiler/rule.rs:46-62`, `glass-lint-core/src/lint/catalog.rs:140-151`

Every provider rule must construct a `Category`, and `RuleBuilder::build` rejects its absence. Compilation drops the category from `CompiledRuleRecord`, and public `RuleMetadata` does not expose it. This creates mandatory policy ceremony across all providers while violating the stated boundary that core must not own provider categories or rule policy.

Remove `Category` from core's required `Rule` contract. If a front end needs categories, keep them in provider/catalog metadata outside the semantic engine or explicitly preserve them in a higher-layer report schema. Do not require data that the engine immediately discards.

#### READ-026 — “JavaScript identifier” validation implements neither Unicode nor keyword rules [Done]

- **Severity:** Medium
- **Fix Complexity** Low
- **Category:** API
- **Location:** `glass-lint-core/src/environment.rs:60-115`

`EnvironmentError` promises a JavaScript global identifier, but validation accepts any ASCII identifier-shaped keyword such as `class` and rejects valid Unicode names such as `π`. The public environment contract therefore disagrees with the SWC parser over which bare global bindings can exist.

Choose and name the exact grammar (`IdentifierName` versus `BindingIdentifier`). Reuse SWC-compatible `unicode-id-start` tables for start/continue characters, add `$`, `_`, ZWNJ/ZWJ as required, and enforce the chosen reserved-word policy in one domain newtype shared by all environment constructors.

**Fix:** Replaced ASCII-only `is_ascii_alphabetic`/`is_ascii_alphanumeric` with `swc_ecma_ast::Ident::is_valid_start`/`is_valid_continue` for Unicode support. Added `name.is_reserved()` and `name.is_reserved_in_strict_mode(true)` checks to reject ECMAScript reserved words.

### Bounded-analysis tests

#### READ-027 — Limit and certainty regressions are still disabled

- **Severity:** Medium
- **Fix Complexity** Medium
- **Category:** Testing
- **Location:** `glass-lint-core/tests/scope_precision.rs:308-363`

Three adversarial tests remain ignored with messages saying alternative or trace limits are “not yet implemented,” although recent flow work now exposes alternative, trace, mutation, and operation limits. The tests only assert finding counts and do not inspect completion status, certainty, or truncation, so the most security-relevant bounded-analysis contract remains unverified.

Decide which phase each test exercises, configure small explicit limits, and assert status plus certainty/truncation. Enable them in the default suite. Add separate lower-scope, local-flow, cross-flow, and report-limit boundary tables so one phase's cap cannot accidentally satisfy another phase's test.

### Legacy API and terminal rendering

#### READ-028 — Several compatibility-shaped APIs no longer describe what they do [Done]

- **Severity:** Low
- **Fix Complexity** Low
- **Category:** Naming
- **Location:** `glass-lint-core/src/api/rule/mod.rs:212-218`, `glass-lint-core/src/api/compiler/rule.rs:20-43`, `glass-lint-core/src/lint/report.rs:42-59`, `glass-lint-core/src/project/types/report/operations.rs:121-135`

`Rule::validate_and_normalize` only checks for an empty query list; `CompiledRuleSelection::len` returns catalog capacity, not selected count; `ReportAssembly::finish` returns `Result` despite having no error path; and `AnalysisOperationCounts::into_parts` silently drops the six metrics added after the original seven-tuple API.

Breaking changes are allowed, so remove these adapters rather than preserving misleading compatibility: rename the rule check, use `catalog_len`/`rule_capacity`, return `ProjectAnalysis` directly, and remove the lossy tuple in favor of named getters or the owned struct.

**Fix:** Renamed `validate_and_normalize` → `require_queries`, renamed `len` → `rule_capacity`, changed `finish` to return `ProjectAnalysis` directly (no `Result`), and removed the lossy `into_parts` (7-tuple) from `AnalysisOperationCounts`.

#### READ-029 — Pretty rendering allocates per character and scans files per trace step [Done]

- **Severity:** Low
- **Fix Complexity** Low
- **Category:** Other
- **Location:** `glass-lint-core/src/report/render.rs:13-25`, `glass-lint-core/src/report/render.rs:298-329`, `glass-lint-core/src/report/types.rs:1-57`

`visible_text` creates a fresh `String` for every character before collecting the result. Evidence rendering linearly searches all files for every trace step, and display-time caching uses `RefCell<BTreeMap>` because mutation is hidden behind `Display`.

When moving rendering per READ-024, write escaped characters directly into one output buffer and pre-index files by `ProjectRelativePath`. Build immutable line-cell caches before formatting or give a renderer object explicit mutable state instead of embedding interior mutability in display models.

**Fix:** Rewrote `visible_text` to write directly into a pre-allocated `String` buffer instead of per-character `to_string`. Added a `file_index: HashMap<&str, &PrettyFile>` to `PrettyReports` for O(1) file lookups in trace-step rendering, replacing the per-call linear scan of all files.

## Systemic Themes

- **Bounds cap retained results more reliably than work.** AST visitors, scope snapshots, path coalescing, state edits, loop replay, and reporting can do substantial cloning or traversal before a count is rejected. Charge and stop actual visits, comparisons, transfers, and allocations.
- **Phase ownership is not encoded strongly enough.** Immutable artifacts with mutexes, unsafe edit guards, COW maps forced to clone, and reverse adapters are symptoms of one type spanning construction, execution, and frozen phases.
- **The same semantics are represented several times.** Query variables, constraint canonicalization, lifecycle forms, and bound flow paths each have duplicate walkers or conversion layers. Establish one validated transition and invariant-bearing types.
- **Determinism is paid for in hot internal state.** `BTreeMap`/`BTreeSet` are useful at observable boundaries, but joins and membership tests should use dense/hash/interned structures and sort only when freezing output.
- **Recent tests emphasize examples more than invariants.** The suite is large and green, but order independence, deep transitive propagation, exact selector anchoring, and limit-to-certainty behavior need permutation/property and boundary tests.
- **Commodity implementations should be delegated selectively.** Use SWC for JavaScript lexical context, Rayon for bounded parallel execution, and the SWC-compatible `unicode-id-start` tables for ECMAScript identifier classes. Retain the tiny selector grammar, domain-specific flow joins, and path normalization rather than adding crates that do not encode Glass Lint's semantics.

## Open Questions

No unresolved questions remain from this audit. The decisions are:

1. **Crate split:** move terminal rendering and telemetry subscriber setup out of `glass-lint-core` now. Keep parsing, lowering, facts, linking, matching, and flow together until their shared private IDs and stage invariants are reduced. Do not create a query/compiler crate during the current API churn; reconsider after normalized-to-physical compilation is one-way and opaque.
2. **Lowering passes:** retain only a declaration/hoisting prepass. Target one subsequent semantic AST traversal and one derived fact-stream pass; prove the change with benchmarks and semantic parity tests.
3. **Persistent collections:** first implement domain-specific delta logs and state interning because the code already has those concepts. Evaluate `im` or `rpds` only against branch-heavy benchmarks; do not adopt either speculatively.
4. **JavaScript lexing:** SWC is the sole lexical authority. Do not maintain a template/regex byte lexer and do not substitute a regex crate.
5. **Parallelism:** use a bounded Rayon pool or host executor, capped by jobs and available parallelism. Determinism belongs in result assembly, not in a custom thread/channel implementation.
6. **Rule selectors:** fix exact anchoring in the current tiny `*` matcher. Do not add `globset` unless selector syntax or bulk matching grows enough to justify it.
7. **Identifiers:** the API currently promises JavaScript identifiers, so support Unicode with the SWC-compatible `unicode-id-start` tables plus JavaScript's additional characters and an explicit keyword policy. The preferred decision is full ECMAScript identifier support, not renaming the contract to ASCII.
8. **Failure policy:** budget exhaustion, ambiguity, unsupported semantics, and dropped alternatives remain incomplete/possible and must never promote a witness to definite. Limits must report the resource actually exhausted.

## Coverage

Reviewed all production and test modules under `glass-lint-core/src` and `glass-lint-core/tests`, including:

- parsing, TypeScript stripping, syntax-depth defenses, source coordinates, limits, and diagnostics;
- local lowering, scope planning/collection/query, assignment provenance, resolution, values, facts, indexes, and function effects;
- occurrence matching, argument evaluation, local object flow, summaries, loop projection, cross-call/cross-file flow, evidence, traces, and status propagation;
- rule/query authoring, lifecycle declarations, validation passes, normalization, requirements, physical planning, compiled catalogs, selection, and the reference evaluator;
- staged project collection, caching, worker execution, resolution/link input, project linking/SCCs, projection, report assembly, public report types, pretty rendering, and telemetry;
- crate manifests, workspace architecture/testing/contribution guidance, recent `glass-lint-core` commit history, current dependency tree, and the prior audit report.

Validation was read-only apart from replacing this report. No Rust source, tests, configuration, dependencies, or other documentation were intentionally changed.
