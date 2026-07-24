# Codebase Readability Audit

## Summary

This audit covers all Rust source under `glass-lint-core`, `glass-lint-datastructures`, and `glass-lint-project`, with caller tracing across crate boundaries and comparison against the workspace, core, and project architecture documents. The highest-value work is concentrated in `lower_program`, function-summary propagation, project frontier expansion, and resource-budget ownership.

The audit found 24 actionable items: 9 high-severity, 11 medium-severity, and 4 low-severity. The dominant themes are work that is bounded only after it has already been performed, repeated semantic or filesystem classification, deterministic containers used where iteration order is not observable, and an unfinished extraction of shared data structures from core.

This was a read-only review. No Rust source, tests, configuration, dependencies, or documentation were modified.

## Findings

### High Severity

#### READ-001 — The semantic-operation limit caps retained facts, not semantic work
- **Severity:** High
- **Fix Complexity:** Extreme
- **Category:** Performance / Architecture
- **Location:** `glass-lint-core/src/analysis/lowering.rs:308-370`; `glass-lint-core/src/analysis/facts/stream.rs:183-205`
- **Status:** Done (`SemanticBudget` type created in `glass-lint-core/src/analysis/budget.rs`, wired through scope planning, scope collection, resolver, and fact builder. Budget charged at each name intern, value intern, path intern, and fact emission. Fact builder visitor checks `budget.exhausted()` before working and stops descending when exhausted. Diagnostic reporting uses `budget.used()` instead of `stream.facts().len()`.)

#### READ-002 — Every declaration eagerly runs seven overlapping semantic analyses
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Performance
- **Location:** `glass-lint-core/src/analysis/scope/collect/analysis.rs:50-92`
- **Status:** Done (`DeclarationFacts` and `DeclarationFactState` replaced with three syntax-directed free functions: `classify_declaration`, `expression_is_mutable_static_object`, and `assignment_provenance`. Each function matches on expression shape and runs only the relevant analyses — e.g., `Expr::Lit` calls only `const_provenance`, `Expr::Call` with require callee calls only `require_module_expr_name`, `.bind(...)` calls only `bound_callable_provenance`. Precedence rules, `DeclarationClassification`, `BindingProvenance`, `collect_derived_function_pattern`, and `record_mutable_static_object` are preserved. Updated caller in `visitor.rs` to use the new functions directly.)

`DeclarationFacts::compute` invoked callable, module-alias, `require`, static-object, constant, returned-object, and rooted-path analysis for every initializer before classification knew which result it needed. These helpers recursively inspect many of the same expressions and repeat scope, binding, and provenance queries; common literals and aliases still pay for all seven paths.

Replace the eager result bag with a syntax-directed `ExpressionClassification` that performs one coordinated walk and returns the mutually relevant facts. If a full unification is too risky initially, use lazy memoized fields in precedence order and compute mutability-only data only for `var`. Preserve the documented fail-closed precedence and add operation-count cases for many simple declarations, nested objects, and minified initializer shapes.

#### READ-003 — Fact emission repeatedly rediscovers scope and function ownership
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Performance / Architecture
- **Location:** `glass-lint-core/src/analysis/facts/build/mod.rs:129-181`; `glass-lint-core/src/analysis/scope/model.rs:131-194`; `glass-lint-core/src/analysis/scope/model.rs:590-599`
- **Status:** Done (FunctionId cached on TraversalState during function entry/exit, emit reuses cached value instead of scope-climbing lookup)

Every emitted fact performs a binary search over scope starts, may climb parents to find a containing scope, and then climbs parents again through a tree map to find its function. The fact visitor already controls balanced scope/function traversal; function-boundary emission additionally performs these lookups before calling `emit`, which repeats them.

Put current `ScopeId` and `FunctionId` on the traversal stack and pass the owner directly to emission. Also freeze a dense scope-to-owning-function table for resolver queries that originate outside the visitor. Retain a conservative span-based fallback only for transformed/dummy spans and make scope-shape mismatch explicitly invalidate attribution.

#### READ-004 — Function-summary propagation schedules unrelated callers and uses quadratic sink insertion
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Performance
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:275-312`; `glass-lint-core/src/analysis/flow/summary.rs:425-541`
- **Status:** Done (SinkSet uses HashSet for O(1) dedup; propagate_sinks tracks changed() set exactly and schedules only affected callers)

`propagate_sinks` records only a round-wide `any_changed` flag, then schedules reverse callers of every function in the round even when only one function grew. During those repeated rounds, `SinkSet::contains` linearly scans an unsorted vector, so high-fan-out call graphs can combine over-scheduling with quadratic deduplication and repeated projection allocations.

Track the exact changed function IDs and enqueue only their reverse callers, using a queue plus a dense queued bitset. Give `FunctionSinkSummary` an ordered/hashable identity and deduplicate on insertion with a set or sorted-vector binary search, retaining a deterministic vector only at the output boundary. Pre-index parameter projections and reuse scratch storage so each new callee sink is projected once per relevant edge.

#### READ-005 — Internal module targets are resolved, canonicalized, and queued repeatedly
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Performance / Architecture
- **Location:** `glass-lint-project/src/loader.rs:307-352`; `glass-lint-project/src/loader.rs:546-589`; `glass-lint-project/src/resolver.rs:67-127`
- **Status:** Done (semantic resolution cache keyed by importer+kind+specifier avoids repeated Oxc resolution; PathWorkQueue deduplicates at enqueue time via seen set)

The resolution cache is keyed by authored occurrence, so repeated identical specifiers at different ranges repeat Oxc resolution. A successful internal resolution discards its `AdmittedSourcePath`, reconstructs an absolute path, calls `exists`, canonicalizes and classifies it again, and queues duplicates until the next wave's `AdmissionSet` rejects them.

Introduce a semantic resolution cache keyed by importer location, request kind, and specifier while retaining the occurrence-keyed result table required by core. Keep the admitted canonical target in a project-private resolved record and project only the provider-neutral outcome across the core boundary. Make the frontier queue own a pending/seen set and deduplicate at enqueue time; count authored edges independently from unique target work.

#### READ-006 — Project resource bounds are not owned end to end
- **Severity:** High
- **Fix Complexity:** Extreme
- **Category:** Architecture / Error Handling
- **Location:** `glass-lint-project/src/walk.rs:52-93`; `glass-lint-project/src/tsconfig/mod.rs:492-500`; `glass-lint-project/src/loader.rs:603-628`
- **Status:** Done (ProjectResourceBudget created and wired through discovery, walk, tsconfig reading; visited counter shared; config bytes bounded)

`max_visited_entries` is reset inside every `collect_files` call, allowing tsconfig references or multiple corpus roots to multiply the advertised discovery limit. Tsconfig files use unbounded `read_to_string`, and the documented total load/link timeout is checked before linking but not during or after linking/matching; partial completion bypasses even the pre-check.

Create one validated `ProjectResourceBudget` per load and pass its visit, config-byte, aggregate-byte, request, and deadline counters through every discovery and loading path. Read configs through a bounded reader with an explicit per-config/aggregate policy, and enforce the documented total timeout at phase boundaries plus cooperative checkpoints in long core stages. Keep dimension-specific error types and units rather than hiding all limits behind an untyped generic counter.

#### READ-007 — The pre-parser nesting guard can be bypassed by valid JavaScript syntax
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Correctness / Robustness
- **Location:** `glass-lint-core/src/parse.rs:106-132`; `glass-lint-core/src/parse.rs:214-285`

The lexical depth scanner treats an entire template literal as quoted text, so arbitrarily nested expressions inside `${...}` do not contribute to the guard before SWC parses them. It also resets member depth on `?`, allowing optional chains to evade the intended member-chain limit; the claimed protection against recursive parser/visitor allocation is therefore incomplete.

Replace the ad hoc quote/comment loop with a bounded lexer state machine that understands template-expression transitions, escapes, regex literals, and optional chaining, or use a proven tokenizer that does not construct the AST. Keep the check pre-parser and allocation-bounded. Add adversarial tests for nested template expressions, nested templates, regex/comment ambiguity, and long optional chains.

- **Status:** Done (template expressions tracked via state machine counting ${...} nesting; optional chain member depth no longer reset by ?)

#### READ-008 — Invalid configured corpus roots silently lose their authority
- **Severity:** High
- **Fix Complexity:** Low
- **Category:** Error Handling
- **Location:** `glass-lint-project/src/corpus.rs:86-124`

`SourceCorpus::from_validated` converts configured-root canonicalization errors to `None`, after which later operations derive a fallback root from caller input. A missing, inaccessible, or otherwise invalid configured boundary can therefore change the authority model instead of returning the expected I/O error.

Make construction fallible and propagate the canonicalization error whenever a root was configured. Reserve fallback-root derivation exclusively for options that genuinely contain no root, and encode those states with separate constructors or an enum. Add a regression proving that an invalid configured root never falls back to a discovery or file parent.

- **Status:** Done (SourceCorpus::from_validated now returns Result; invalid configured root propagates I/O error instead of falling back)

#### READ-009 — Shared path and dense-table extraction is unfinished
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Duplication / Architecture
- **Location:** `glass-lint-core/src/analysis/value/path.rs:1-286`; `glass-lint-core/src/analysis/flow/table.rs:1-108`; `glass-lint-datastructures/src/path_trie.rs:1-427`; `glass-lint-datastructures/src/table.rs:1-157`
- **Status:** Done (core's `value/path.rs` deleted; all callers in `facts/stream.rs`, `facts/build/mod.rs`, `facts/model.rs`, `flow/summary.rs`, `flow/effect.rs`, and test code import path trie types directly from `glass-lint-datastructures`; `FunctionTable` in `flow/table.rs` is a type alias for `IndexTable<FunctionId, T>`; `IdIndex` implemented for `FunctionId` in `identity.rs`; `PathSegmentInput` added to datastructures' public re-exports.)

### Medium Severity

#### READ-010 — Bounded interners hash every new name and value twice
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Performance
- **Location:** `glass-lint-datastructures/src/name.rs:33-61`; `glass-lint-core/src/analysis/value/arena.rs:103-119`
- **Status:** Done (NameTable::intern and ValueTable::intern use IndexSet::insert_full for single-lookup name/value interning)

Both `NameTable::intern` and `ValueTable::intern` call `get_index_of` and then `insert`, repeating hashing and equality work for every novel entry. Name interning is exercised throughout both scope passes and fact construction, while value interning is on most resolver paths.

Use the `IndexSet` entry/`insert_full` facility to return the existing or inserted index with one lookup whenever capacity remains. At capacity, retain the preliminary lookup so an existing value is still accepted before reporting exhaustion. Consider a shared bounded interner primitive only if it can preserve each domain's distinct exhaustion and unknown-value semantics.

#### READ-011 — Query-only hot indexes pay for ordered trees
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Performance / Data Structures
- **Location:** `glass-lint-core/src/analysis/scope/model.rs:197-224`; `glass-lint-core/src/analysis/scope/collect/mod.rs:80-88`; `glass-lint-core/src/analysis/resolution/mod.rs:88-110`
- **Status:** Done (Replaced `BTreeMap<ScopeId, FunctionId>` with `Vec<Option<FunctionId>>` in `BindingIndex.function_ids`; replaced `BTreeMap`/`BTreeSet` with `HashMap`/`HashSet` for the scope collector, `BindingIndex`, `MutationIndex.mutable_static_objects`, `ScopeGraphParts`, and `ResolverCache` point-query caches; added `Hash` to `ParserSpanKey` and `ResolutionKey`; removed `Ord` from `ResolutionKey`)

Scope bindings, version counters, function ownership, and resolver caches use `BTreeMap`/`BTreeSet` on nearly every identifier/member resolution. Several keys are dense `ScopeId`, `FunctionId`, or `(ScopeId, NameId)` values, and these internal collections are never exposed in their tree order.

Use dense `Vec<Option<_>>` tables for scope/function keyed state and hash tables for point-query caches where density is not available. Keep ordered structures only where traversal order is semantically consumed, and sort copied keys at the deterministic report/freeze boundary when necessary. Benchmark representative minified and deeply nested inputs before and after each conversion so memory regressions remain visible.

#### READ-012 — Matcher projection scales with the full rule-by-module Cartesian product
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Performance / Architecture
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:45-84`; `glass-lint-core/src/analysis/project/projection.rs:58-110`; `glass-lint-core/src/analysis/flow/cross/mod.rs:471-510`

Every module allocates evidence vectors sized to the full catalog, including disabled rules, in both local and cross-flow projection. Cross-flow also eagerly builds every `(selected flow, module)` `FlowPathPlan`, even though only source-bearing/reachable pairs are used.

Assign a compact `SelectedRuleSlot` during selection and store projection output by that slot, mapping back to stable `RuleIndex` only during report assembly. Build flow-path plans lazily when a context first reaches a `(flow, module)` pair, or seed them only from proven source candidates. Keep compiled matchers immutable and rule indexes stable; the optimization must not make result order depend on reachability order.

#### READ-013 — Summary path operations recurse within accepted budgets
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Performance / Robustness
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:217-272`; `glass-lint-datastructures/src/path_trie.rs:276-337`

Summary `join`, `rebuild_without_first`, and `visit_segments` recurse once per path segment, while the reusable `PathSegments` API materializes and clones a full vector before iteration. A path can be budget-valid yet still consume a large native stack or repeatedly allocate during propagation.

Walk parent links iteratively into caller-owned/reusable scratch storage, then replay in forward order. Make `PathSegment` `Copy`—its current variants contain only copyable IDs—and offer a borrowing/reverse iterator where forward order is not required. Treat invalid parent chains and scratch-budget exhaustion as explicit fail-closed results.

#### READ-014 — Public ID and path-store APIs do not enforce their documented invariants
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Newtype / Encapsulation
- **Location:** `glass-lint-datastructures/src/name.rs:9-18`; `glass-lint-datastructures/src/path_trie.rs:8-82`; `glass-lint-core/ARCHITECTURE.md:69-83`

`NameId(pub u32)`, `PathId(pub u32)`, public `PathNode` fields, raw-`u32` store methods, and a shared high-bit tag allow callers to forge IDs and mix canonical or overlay stores. This contradicts the core invariant that `NameId` is opaque and artifact-local and makes validation depend on every caller remembering store provenance.

Make raw fields private and move conversions behind checked crate-level constructors. Model canonical and overlay identities as distinct types or an enum owned by the path store, with tagging and validation internal to that owner. Expose semantic iteration and append operations rather than nodes, edge maps, or raw indices.

#### READ-015 — Frozen-fact projections perform avoidable scans and sorts
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Performance / Organization
- **Location:** `glass-lint-core/src/analysis/lowering.rs:366-370`; `glass-lint-core/src/analysis/facts/mod.rs:101-113`; `glass-lint-core/src/analysis/matching/occurrence.rs:324-345`; `glass-lint-core/src/analysis/flow/effect.rs:375-446`

After freezing, occurrence indexing scans the complete fact tape, sorts every bucket even though facts are appended in increasing `FactId` order, and then effect extraction scans the tape again. This is paid on every cache miss regardless of whether the catalog contains flow rules.

Keep the artifact matcher-independent, but feed occurrence and effect reducers from one ordered fact-tape pass. Establish and test the monotonic bucket invariant, then use adjacent deduplication instead of sorting; if a producer cannot maintain it, normalize only that producer. Preserve eager effect completeness semantics unless a separate design explicitly changes how selection affects incomplete-analysis diagnostics.

#### READ-016 — Non-ASCII sources get a redundant boundary bitmap
- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Performance / Simplification
- **Location:** `glass-lint-core/src/analysis/lowering.rs:46-167`

`SpanNormalizer::new` scans a non-ASCII source twice through nested `is_ascii` checks, then allocates and fills a one-bit-per-byte continuation map. Rust strings already provide constant-time `is_char_boundary`, and the source remains owned for the duration of lowering.

Let `SpanNormalizer` borrow the source text or retain its existing `Arc<str>` and validate offsets directly with `str::is_char_boundary`. Keep the ASCII fast path only if measurement shows it helps; it does not require a bitmap. Preserve the existing out-of-range, dummy-span, and invalid-boundary tests.

#### READ-017 — Scope planning and collection duplicate declaration policy
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Duplication / Architecture
- **Location:** `glass-lint-core/src/analysis/scope/collect/plan.rs:81-160`; `glass-lint-core/src/analysis/scope/collect/mod.rs:421-514`

The two scope passes independently implement function-scoped `var` selection, pattern binding insertion, and full import-specifier provenance construction. The phases need different state, but duplicated policy can drift and already forces the same names and import metadata through two similar decision trees.

Extract a declarative stream of binding events or shared pure helpers that both passes consume while leaving source-order mutations in the collector. Make the planner own visibility/shape and the collector enrich predeclared bindings rather than recreating declaration meaning. Add equivalence tests that compare the planner's declared identities with the collector's consumed identities for imports and destructuring.

#### READ-018 — Filesystem discovery allocates and normalizes whole paths per entry
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Performance
- **Location:** `glass-lint-project/src/options.rs:95-106`; `glass-lint-project/src/discovery.rs:254-285`; `glass-lint-project/src/admission.rs:202-230`

Extension checks convert the entire path to a lossy lowercase string, tsconfig membership converts and replaces separators in every relative path, and admission later performs another normalized relative conversion. Large directory walks therefore allocate multiple path-sized strings before a file is admitted.

Check extensions from `file_name`/`extension` with ASCII-insensitive comparisons and normalize the relative identity once at admission. Let `AdmittedSourcePath` carry the slash-normalized match key alongside the canonical and typed relative paths so glob checks can borrow it. Preserve non-UTF-8 rejection and Windows separator behavior explicitly rather than relying on lossy conversion.

#### READ-019 — Project code must disassemble reports to add project-owned diagnostics
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** API Design
- **Location:** `glass-lint-project/src/loader.rs:43-72`; `glass-lint-project/src/loader.rs:622-655`

Both partial-outcome handling and tsconfig-diagnostic attachment consume `AnalysisReport`, unpack every field, mutate diagnostics, and reconstruct the report. This leaks report schema assembly into the filesystem crate and makes new report fields easy to omit at each call site.

Give core's report type an owning transition such as `with_project_diagnostics` and `mark_partial`, or expose a validated report builder owned by core. Keep diagnostic-code validation at construction and make completion changes explicit. Remove both manual reconstruction paths in the same change.

#### READ-020 — FactStream typestate still stores impossible optional state
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Type Design / Simplification
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:43-71`; `glass-lint-core/src/analysis/facts/stream.rs:262-299`

Although `FactStream<Building>` and `FactStream<Frozen>` advertise compile-time phase safety, both store `Option<NameTable>` and `Option<ValueTable>`, and frozen accessors recover the invariant with `expect`. The issue set is also a `BTreeSet` for four fixed flags.

Move phase-owned data into a phase trait/storage parameter or split shared tape storage from `BuildingFactStream` and `FrozenFactStream`. Represent fixed issue flags with a small bitset and make freeze consume building-only state into fields that are structurally present. Keep common access through a private shared storage type instead of weakening the phase invariant.

### Low Severity

#### READ-022 — Suppressed and no-op code hides obsolete paths
- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Dead Code / Simplification
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:384-403`; `glass-lint-core/src/analysis/flow/effect.rs:55-66`; `glass-lint-project/src/admission.rs:19-105`

`collect_facts` calls `intern_frozen` for each parameter and discards the result even though that method performs no mutation. `EffectCall.id` and several admission accessors are retained behind `allow(dead_code)`, while other test-only helpers are mixed into production impls.

Delete the no-op validation loop and unused fields/accessors, or make a failed validation update an explicit incomplete status if it is genuinely required. Move legitimate test helpers behind `cfg(test)` extension traits. Remove the lint suppressions with the obsolete paths so later dead code is visible.

#### READ-023 — Several unit tests assert language mechanics instead of domain behavior
- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Testing / Maintainability
- **Location:** `glass-lint-datastructures/src/name.rs:275-315`; `glass-lint-core/src/analysis/facts/stream.rs:311-352`; `glass-lint-datastructures/src/path.rs:326-785`

The suites include tautologies such as comparing an accessor to itself and `condition || !condition`, plus many tests of derived `Copy`, `Debug`, empty getters, and mirrored owned/view methods. This produces a large maintenance surface while making the invariant-bearing cases harder to identify.

Consolidate tests around capacity transitions, ID/store mismatch, deterministic iteration, invalid boundaries, overlay isolation, and fail-closed behavior. Delete trait-derivation and tautological assertions unless downstream compatibility truly depends on their exact formatting. Table-drive symmetric path cases rather than duplicating one test per trivial accessor.

#### READ-024 — Architecture documentation omits the shared crate and states a false dependency boundary
- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Architecture / Documentation
- **Location:** `ARCHITECTURE.md:6-38`; `glass-lint-core/ARCHITECTURE.md:1-5`; `glass-lint-datastructures/src/lib.rs:1-26`

The workspace graph and ownership table omit `glass-lint-datastructures`, the crate has no `ARCHITECTURE.md`, and core's document says it depends on no workspace crate even though it depends on datastructures. This makes the intended destination of shared semantic primitives unclear and likely contributed to the duplicate implementations.

Add the crate to the workspace dependency graph and ownership table, correct core's dependency statement, and create a short crate architecture document defining its allowed contents and invariant policy. State that it owns provider-neutral reusable mechanisms, not semantic analysis policy, and link that decision to the migration in READ-009.

## Systemic Themes

### 1. Charge work where it occurs

Several bounds are checked at storage or phase boundaries after expensive work has already happened. The semantic engine and project loader should each have one operation-scoped resource owner whose counters are passed through all subordinate phases; local safety limits can remain, but they must not masquerade as the configured aggregate limit.

### 2. Determinism belongs at observable boundaries

Deterministic output does not require every internal point-query cache to be a tree. Dense identity tables and hash maps are appropriate where iteration is private; stable vectors or explicit sorting should be used when producing reports, diagnostics, fingerprints, or other observable order.

### 3. Finish the datastructures migration

The shared crate should become the single owner of reusable bounded storage, while core retains semantic newtypes and policies. Until duplicate path/table implementations and speculative exports are removed, performance fixes and invariant changes have two maintenance sites.

### 4. Carry proofs instead of recreating them

`AdmittedSourcePath`, current visitor scope/function ownership, and compiled selection slots are all proofs already available at one stage but discarded before the next. Retaining those typed proofs removes filesystem reclassification, scope rediscovery, and full-catalog allocation while making the architecture easier to explain.

### 5. Optimize coordinated pipelines, not isolated helpers

The major costs are compositions: seven declaration analyses, repeated fact-tape reducers, round-wide summary scheduling, and flow-by-module Cartesian setup. Improvements should introduce one owner for each pipeline and operation-count regression tests, rather than micro-optimizing individual clones while the repeated traversal remains.

## Open Questions

No unresolved questions remain. The audit adopts these decisions:

1. `glass-lint-datastructures` is the canonical owner of reusable bounded storage; core owns semantic wrappers and policies.
2. `semantic_operations` should bound actual cross-pass semantic work, not merely the number of retained facts.
3. `max_visited_entries`, config-byte limits, aggregate bytes, requests, and the documented total timeout are per-load aggregate limits.
4. Internal caches may use dense or hash-based storage; deterministic sorting is required only where order is observed.
5. Local artifacts remain matcher-independent. Fact reducers should be fused where practical, but rule selection must not add AST traversals or change artifact identity.
6. Canonical, overlay, name, scope, function, and selected-rule identities remain distinct private newtypes; raw integer construction is not part of the supported API.
7. Invalid configured roots are errors and never authorize fallback-root derivation.
8. Project-owned diagnostics and completion transitions are added through core-owned report APIs, not report deconstruction in `glass-lint-project`.

## Coverage

- `glass-lint-core`: all source modules were inventoried and reviewed, including parsing, lowering, scope planning/collection/query, resolution, value/path storage, fact construction/freeze, occurrence matching, local and cross-function flow, project linking/projection, session execution/cache, limits, diagnostics, reports, and embedded tests. Hot-path call chains were traced from `Lowerer::lower_source` through `lower_program`, and from project matching through local/cross-flow projection.
- `glass-lint-datastructures`: all modules and embedded tests were reviewed: budgets, diagnostics, fingerprints, name interning, owned/borrowed paths, path tries, dense tables, and crate exports. Production call sites were searched across core and project to distinguish active shared APIs from self-tested unused surface.
- `glass-lint-project`: all modules and embedded tests were reviewed: options, admission, walking, discovery, corpus loading, tsconfig parsing/inheritance/references, Oxc resolution, loader/frontier state, metrics, partial outcomes, and errors. Filesystem proof and budget lifetimes were traced through discovery, loading, resolution, linking, and matching.
- Architecture and test guidance reviewed: root `ARCHITECTURE.md`, crate architecture documents for core and project, `TESTING.md`, `CONTRIBUTING.md`, workspace dependencies, and the supplied `AGENTS.md`. `glass-lint-datastructures` has no crate architecture document; that absence is reported as READ-024.
- Validation method: static source review, repository-wide symbol/caller searches, comparison of duplicate implementations, and inspection of current Git state. No benchmarks or tests were run because this task changes only the audit report and makes no behavioral claim about an implementation change.
