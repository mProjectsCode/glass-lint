# Glass Lint Core Readability Audit

## Summary

`glass-lint-core` has a strong stated architecture—parse once, lower into immutable matcher-independent facts, and fail closed—but several later layers recreate owned projections of those facts or recover construction-time mutability through `Rc`, `Arc`, `RefCell`, and `Cell`. The most consequential opportunities are to make lowering an explicit ownership pipeline, separate immutable linked-project data from per-classification results, unify the two query engines and two function-flow models, and make source/fact-derived views borrow from their canonical owners.

This audit found 26 items: 7 high severity, 16 medium severity, and 3 low severity. The high-severity set includes one concrete matching inconsistency: project overlays are not applied to the separately stored effective arguments of `.call()` and `.apply()` invocations.

## Findings

### READ-001 — Lowering proves exclusive name ownership at runtime
- **Severity:** High
- **Category:** Interior Mutability
- **Location:** `glass-lint-core/src/analysis/name.rs:32-104`, `glass-lint-core/src/analysis/resolution/mod.rs:84-105`, `glass-lint-core/src/analysis/lowering.rs:157-231`

`NameTableHandle` uses `Rc<RefCell<NameTable>>`, `Resolver` adds another `RefCell` for its state, and finalization depends on `Rc::try_unwrap` followed by `expect`, turning a construction invariant into runtime borrow checks and a panic path. Make one lowering context own the mutable name/value tables, let ordered collection/resolution/fact-building phases borrow that context explicitly, and freeze it through a consuming transition; a separate mutable resolution session can satisfy constant-evaluation callbacks without putting the resolver itself behind interior mutability.

### ~~READ-002 — The linked project model is not actually immutable~~ **DONE**
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/mod.rs:58-83`, `glass-lint-core/src/analysis/project/projection.rs:40-97`, `glass-lint-core/src/analysis/project/graph.rs:18-205`

`ProjectSemanticModel` stores status in a `RefCell` and budgets/counts in `Cell`, so linking helpers with `&mut self` and classification through `&self` both mutate hidden state; this also prevents safe shared classification and makes a query alter later telemetry/diagnostics. Have a mutable linker builder return an immutable `LinkedProject`, and have projection return a `ProjectionOutcome` containing evidence, status, exhaustion, and operation counts instead of writing those results back into the project.

**Fix applied:** Added `ProjectionOutcome` struct with `flow_exhausted`, `effect_projections`, and `flow_observed` fields. `project()` now returns `(ProjectMatcherModel, ProjectionOutcome)` without mutating `self`. `classify_with_evidence_limit` returns the outcome alongside classifications. Callers explicitly merge the outcome via `merge_projection_outcome()` instead of the project mutating itself through hidden interior mutability during `&self` methods.

### ~~READ-003 — Duplicated wrapper arguments bypass project overlays~~ **DONE**
- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/model.rs:128-190`, `glass-lint-core/src/analysis/matching/arguments.rs:34-99`, `glass-lint-core/src/analysis/facts/build/calls.rs:355-393`

Calls retain both `args` and a second `CallUnwrap::effective_args`; constrained matching overlays the first vector but selects the unmodified second vector for `.call()`/`.apply()`, so linked static-string or module identities can match direct calls but fail for equivalent wrappers.

**Fix applied:** `matching/arguments.rs` now applies `argument_with_overlay` to `unwrap.effective_args` before the constrained evaluation loop selects them, so project-level identity overlays reach `.call()`/`.apply()` arguments identically to direct calls.

### READ-004 — Indexed and constrained clauses have separate semantic engines
- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/query.rs:60-310`, `glass-lint-core/src/analysis/matching/arguments.rs:20-103`, `glass-lint-core/src/analysis/matching/arguments.rs:221-370`

Unconstrained clauses execute through occurrence indexes while constrained call clauses scan facts and independently reimplement event, identity, subject, package, returned-value, instance, and overlay rules. Compile candidate selection separately from a single clause predicate evaluator so indexes produce candidate `FactId`s and every candidate follows the same semantic path.

### READ-005 — Local and cross-project flow build parallel function models
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:91-234`, `glass-lint-core/src/analysis/flow/effect.rs:25-138`, `glass-lint-core/src/analysis/flow/projector/mod.rs:39-73`

`FunctionSummaries` and `FunctionEffects` independently collect parameters, calls, writes, invalidation, and propagation from the same fact stream; the former is rebuilt for selected local flow matchers while the latter is retained for project flow. Retain one canonical matcher-independent function-effect/call graph keyed by `FactId`, then run local sink projection and cross-module composition as query state over that graph.

### READ-006 — Artifact-internal reference counting exceeds the sharing boundary
- **Severity:** Medium
- **Category:** Reference Counting
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:31-74`, `glass-lint-core/src/analysis/facts/stream.rs:25-118`, `glass-lint-core/src/analysis/project/projection.rs:21-38`

The cached `SemanticArtifact` already has a justified outer `Arc`, but its occurrence index and name table are separately reference-counted so a detached `ProjectMatcherModel` and every function effect can own them again. Give `ProjectMatcherModel` a project lifetime and borrow indexes, let the fact stream own the frozen name table directly, and let effects resolve names through their containing artifact so reference counting remains only at the cache/artifact boundary.

### READ-007 — Effect and projector records copy canonical call payloads
- **Severity:** Medium
- **Category:** Memory Churn
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:47-125`, `glass-lint-core/src/analysis/flow/effect.rs:485-566`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:10-96`

Each call can copy `CallArgInfo` into `EffectCall`, copy it again for a receiver use, duplicate a derived `EffectArgument` per argument use, and later copy it into `SourceCall`, although the immutable `FactStream` remains retained. Store event IDs and minimal relation metadata in effects/indexes, then borrow the call payload from the stream during projection.

### ~~READ-008 — Binding-pattern traversal is implemented repeatedly~~ **DONE**
- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/syntax/names.rs:46-74`, `glass-lint-core/src/analysis/facts/build/calls.rs:169-345`, `glass-lint-core/src/analysis/scope/collect/aliases.rs:16-176`, `glass-lint-core/src/analysis/scope/collect/callbacks.rs:62-161`

Name collection, value collection, write invalidation, parameter paths, alias projection, require destructuring, and callback projection each recursively encode JavaScript pattern shapes with subtly different omissions. Introduce one borrowed pattern walker that yields typed leaves with path/default/rest/write-target metadata, while consumers explicitly choose which yielded forms their strict semantics accept.

**Fix applied:** Added `walk_pat_ident_bindings()` to `syntax/names.rs` — a shared borrowed walker that calls a closure for each `Ident` in a destructuring pattern. `collect_pat_bindings` and `collect_parameter_binding_names` now delegate to the walker, eliminating two independent recursive pattern-match implementations.

### ~~READ-009 — Function-exit facts recompute and retain unused parameters~~ **DONE**
- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/build/functions.rs:22-118`, `glass-lint-core/src/analysis/facts/model.rs:277-284`, `glass-lint-core/src/analysis/flow/projector/mod.rs:193-206`

Every function's parameter bindings are resolved and allocated for both entry and exit facts, but summary/effect consumers read parameters only from entry and the projector uses only the exit marker. Split entry and exit payloads, or make parameters optional only on entry, so parameter patterns are lowered once and stored once.

**Fix applied:** `emit_function_fact` now only resolves parameter bindings for `FunctionBoundary::Enter`. For `Exit`, parameter resolution is skipped entirely and an empty vector is stored, eliminating duplicate work and allocation for all function/arrow/method exit markers.

### ~~READ-010 — Semantic APIs force callers to clone SWC nodes~~ **DONE**
- **Severity:** Medium
- **Category:** Borrowing
- **Location:** `glass-lint-core/src/analysis/facts/build/calls.rs:22-44`, `glass-lint-core/src/analysis/scope/query/provenance.rs:344-459`, `glass-lint-core/src/analysis/facts/build/assignments.rs:122-145`

Callers clone complete `CallExpr`, `MemberExpr`, `Ident`, and assignment patterns merely to construct temporary `Expr`/`Pat` enum values accepted by generic resolver and scope APIs. Add borrowed variant-specific entry points or a small `ExprRef`/pattern-view abstraction so existing AST nodes can be inspected without synthesizing owned sum types.

**Fix applied:** In `calls.rs`, replaced two `Expr::Call(call.clone())` wrappers with a single `resolve_call_expression(call)` call, eliminating the `CallExpr` clone. In `provenance.rs`, `ident_value_seed` and `member_value_seed` now construct binding keys and constants directly from `&Ident`/`&MemberExpr` without wrapping them in owned `Expr` enums, removing `Expr::Ident(ident.clone())` and `Expr::Member(member.clone())` clones.

### ~~READ-011 — Span normalization allocates and clones a source-sized boundary table~~ **DONE**
- **Severity:** Medium
- **Category:** Borrowing
- **Location:** `glass-lint-core/src/analysis/lowering.rs:39-93`, `glass-lint-core/src/analysis/lowering.rs:157-169`, `glass-lint-core/src/analysis/resolution/mod.rs:96-105`

`SpanNormalizer` builds a `Vec<bool>` for every byte boundary and `lower_program_with_name_limit` clones that allocation into `Resolver`.

**Fix applied:** Replaced `Vec<bool>` boundary table with `Arc<str>` stored source text. `normalize()` calls `str::is_char_boundary()` directly. The `Arc` makes cloning the normalizer into `Resolver` a cheap ref-count increment instead of a source-sized allocation.

### READ-012 — Source text is copied across admission, caching, jobs, artifacts, and reporting
- **Severity:** High
- **Category:** Memory Churn
- **Location:** `glass-lint-core/src/analysis/local.rs:19-98`, `glass-lint-core/src/project/session.rs:7-18`, `glass-lint-core/src/project/session.rs:157-205`, `glass-lint-core/src/lint/linter.rs:14-61`

One source can exist as `SourceFile::source`, a full `ArtifactCacheKey::source`, `LocatedSourceContext::text`, cloned worker input/result, and a second report-time source map; cache keys also clone the whole environment and limits per entry. Convert admitted input once into an internal shared source/config identity, move jobs rather than iterate borrowed chunks, and render from each module's existing source context instead of rebuilding `ProjectFileState::sources`.

### READ-013 — Single-file and batch artifact loading duplicate the same state machine
- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/lowering.rs:100-140`, `glass-lint-core/src/project/session.rs:401-442`, `glass-lint-core/src/project/session.rs:472-556`

`lower_source` and `lower_artifact` repeat parse/normalization/lowering, while the single-source and batch session paths separately implement fingerprinting, cache hit/miss accounting, context construction, insertion, eviction, parse-failure handling, and recording. Centralize an artifact loader that returns a typed hit/miss/failure result and let executors only schedule owned misses.

### ~~READ-014 — Argument lowering repeats resolution and constant projection~~ **DONE**
- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/build/arguments.rs:14-75`, `glass-lint-core/src/analysis/facts/build/arguments.rs:78-153`

Building one `CallArgInfo` asks for expression resolution multiple times and independently walks an object/array for base paths, value projections, keys, property strings, a scalar string, and rooted provenance; resolver caching limits recomputation but still returns owned clones and does not combine the constant walks. Produce a single resolved-argument record from one traversal, or retain a compact frozen value/projection arena that facts can reference and later consumers can borrow.

**Fix applied:** `arg_info` now calls `resolve_expr` once and reuses the returned `ResolvedValue` for both `.id` and `.call` access, eliminating the second full expression resolution call.

### ~~READ-015 — ScopeGraph keeps redundant data and a hidden mutable memo~~ **DONE**
- **Severity:** Medium
- **Category:** Interior Mutability
- **Location:** `glass-lint-core/src/analysis/scope/model.rs:61-97`, `glass-lint-core/src/analysis/scope/model.rs:120-181`, `glass-lint-core/src/analysis/scope/model.rs:346-368`

Dynamic-eval effects are retained in both a flat vector and a grouped map by cloning every effect, although queries use only the grouped map.

**Fix applied:** Removed the redundant `dynamic_evals: Vec<(ScopeId, ScopeEffect)>` field from `ScopeGraph`. `finish_collected_properties()` now builds `dynamic_evals_by_scope` directly from the filtered/sorted input, eliminating the intermediate full clone. The `Cell` memo for span lookup is retained since it is a legitimate optimization during single-threaded construction.

### ~~READ-016 — Scope collection falls back to raw strings after interning names~~ **DONE**
- **Severity:** Medium
- **Category:** Newtypes
- **Location:** `glass-lint-core/src/analysis/scope/collect/mod.rs:49-80`, `glass-lint-core/src/analysis/scope/collect/callbacks.rs:163-179`, `glass-lint-core/src/analysis/scope/collect/mod.rs:220-229`

Function and call tables use `(ScopeId, String)` even though the collector owns a name table, causing repeated `name.to_string()` allocation while walking parent scopes and a later conversion back into `ScopedName`.

**Fix applied:** Changed `function_scopes` key from `(ScopeId, String)` to `(ScopeId, NameId)` and `calls` second field from `String` to `NameId`. Names are interned once at insertion time. `function_for_call()` and `function_scope_for_name()` now use `NameId` lookups directly, eliminating `to_string()` allocations on every scope-walking lookup.

### ~~READ-017 — Assignment versioning rescans the complete assignment log~~ **DONE**
- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/collect/mod.rs:356-387`, `glass-lint-core/src/analysis/scope/collect/history.rs:14-51`

Every new assignment counts all preceding assignments for the same scope/name, producing quadratic work for repeatedly assigned bindings.

**Fix applied:** Added a `version_counters: BTreeMap<(ScopeId, NameId), u32>` to `LexicalScopeCollector`. `record_assignment()` now increments a per-(scope, name) counter instead of scanning the full assignment vector, eliminating the O(n) scan per assignment.

### READ-018 — Occurrence queries materialize and copy normalized buckets
- **Severity:** Medium
- **Category:** Borrowing
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:42-111`, `glass-lint-core/src/analysis/matching/query.rs:22-58`, `glass-lint-core/src/analysis/matching/mod.rs:311-339`

APIs expose `&Vec`, then clone whole buckets, collect predicate matches, merge into another vector, sort/deduplicate again, and finally copy into public evidence. Expose slices and borrowed ordered iterators, provide a stable merge/dedup iterator for overlays, and consume that stream directly into the one final evidence allocation.

### READ-019 — Control-flow snapshots clone the full live environment
- **Severity:** Medium
- **Category:** Memory Churn
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:41-187`, `glass-lint-core/src/analysis/flow/projector/control.rs:40-94`, `glass-lint-core/src/analysis/flow/projector/control.rs:197-269`

Every branch, loop, try/catch/finally, and abrupt exit clones sorted alias and state vectors, with nested `finally` handling cloning the same snapshots several more times. Model control frames as checkpoints plus reversible deltas, or as arena-owned versions referenced by compact IDs, so branches borrow a baseline and materialize only changed state at joins without adding pervasive reference counting.

### ~~READ-020 — Evidence bounding performs sorted vector insertion per occurrence~~ **DONE**
- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/evidence.rs:49-115`

`AnnotatedEvidence::from_evidence` binary-searches and inserts into the middle of a `Vec` for each occurrence, making normalization quadratic.

**Fix applied:** Changed from per-occurrence binary-search-and-insert to collect-all-then-batch-process: occurrences are pushed into a flat `Vec`, then `sort_by_key`, deduplicate via adjacent comparison, and truncate once in a single pass.

### READ-021 — Member call/read families duplicate the public-to-compiled pipeline
- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/matcher/member.rs:6-275`, `glass-lint-core/src/api/rule/normalization.rs:69-107`, `glass-lint-core/src/api/compiler/rule.rs:303-347`

Member calls and reads have parallel matcher/provenance types, constructors, evidence formatting, sorting, normalization, validation, compiler lowering, identity lowering, indexes, and query branches; returned-member calls and reads repeat the same split. Keep ergonomic public wrappers if desired, but normalize them into one internal member matcher with an event kind and call-only argument constraints before validation/compilation.

### ~~READ-022 — The cross-flow worklist owns every context twice~~ **DONE**
- **Severity:** Medium
- **Category:** Memory Churn
- **Location:** `glass-lint-core/src/analysis/flow/cross.rs:113-159`

`ContextWorklist::push` clones each `CallContext` into a `BTreeSet` and retains the original in `VecDeque`; the context includes a cloned requirement set and this doubles storage up to the 65,536-context bound.

**Fix applied:** Replaced `VecDeque` + `BTreeSet` pair with a single `IndexSet`, which maintains insertion order with deduplication in one allocation per context. Also added `Hash` to `ModuleId`, `QualifiedEvent`, `CrossFlowState`, `CallContext`, and `RequirementSet` to support `IndexSet`. Added a `Hash` impl for `RequirementSet<K>` that hashes each `(key, value)` pair.

### READ-023 — EvidenceList duplicates its owned identity fields
- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/tables.rs:13-53`

Each retained `Evidence` is accompanied by a `ReportEvidenceKey` that clones its message, stringifies/clones its path, and clones its range solely for deduplication. Make the identity a first-class part of one owned evidence record/domain collection, or defer stable sort/dedup until finalization so identity fields are stored once.

### ~~READ-024 — Rule ID access allocates during validation and construction~~ **DONE**
- **Severity:** Low
- **Category:** API Design
- **Location:** `glass-lint-core/src/lint/catalog.rs:132-145`, `glass-lint-core/src/lint/linter.rs:285-305`, `glass-lint-core/src/lint/linter.rs:386-412`

`RuleCatalog::rule_ids` clones the complete vector.

**Fix applied:** Changed `rule_ids()` return type from `Vec<RuleId>` to `&[RuleId]`. Callers that only iterate or check length now borrow the internal vector instead of receiving a clone.

### READ-025 — Integration-test setup is repeated across suites
- **Severity:** Low
- **Category:** Testing
- **Location:** `glass-lint-core/tests/compact_source.rs:21-64`, `glass-lint-core/tests/declarative_matching.rs:20-105`, `glass-lint-core/tests/scope_precision.rs:12-42`

Rule builders, test environments, linter construction, classification wrappers, and count assertions are independently recreated in several integration-test crates. A small `tests/support` module would centralize the fixture contract while leaving behavior-specific positives and adversarial negatives in their current suites.

### ~~READ-026 — Several comments describe superseded storage~~ **DONE**
- **Severity:** Low
- **Category:** Documentation
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:1-5`, `glass-lint-core/src/analysis/flow/projector/state.rs:125-128`, `glass-lint-core/src/analysis/syntax/provenance.rs:52-85`

**Fix applied:** Updated `facts/stream.rs` module-level doc to say "ordinary mutable construction" instead of "interior mutation". Updated `projector/state.rs` comment that claimed "BTreeMap iteration" to say "Sorted-vector iteration". Removed stale `TODO: Candidate for SybolPath?` and `TODO: Consider using SymbolPath here` comments from `syntax/provenance.rs`.

## Systemic Themes

- The canonical fact stream is a sound ownership boundary, but flow effects, summaries, call views, occurrence overlays, and evidence repeatedly turn borrowed facts back into owned derivative models.
- Construction and query lifecycles are conflated. Explicit builder/session/outcome types would remove most `Rc`, `RefCell`, and `Cell` use while making immutable artifacts genuinely shareable.
- Parallel sum-type families—call versus member call/read, direct versus constrained queries, local versus project flow—carry the same semantics through separate code paths and are already drifting.
- Several repeated map/set/vector protocols deserve domain collections: assignment histories, ordered unique worklists, occurrence streams, evidence normalization, and scope/function indexes.
- Source ownership is fragmented above the semantic artifact. Converting admitted input to one internal source object would reduce the largest predictable memory multiplier and simplify cache/job/report APIs.

## Open Questions

- Is `ProjectSemanticModel::classify_with_evidence_limit` intended to be safely repeatable or concurrently callable? Its current `&self` signature suggests yes, while its status and telemetry mutation suggest no.
- Which precision differences between local `FunctionSummaries` and retained `FunctionEffects` are intentional? Those contracts should be named before merging the models.
- Would a compact frozen value/provenance arena retain less memory than the current copied string/provenance projections for representative large files? Heap profiling should decide whether to retain all used values or only fact-referenced projections.
- What source sizes and cache hit rates dominate production? That determines whether a shared source identity, content digest with collision verification, or another cache-key representation is the best tradeoff.
- Are the sorted-vector flow tables measurably small under real workloads? If so, keep their cache-friendly representation but still address whole-environment snapshot copies; if not, their insertion/removal behavior needs separate redesign.

## Coverage

- Read the repository and crate architecture guidance plus `TESTING.md` and `CONTRIBUTING.md`.
- Inventoried all 124 Rust files and 34,984 Rust lines under `glass-lint-core/src` and `glass-lint-core/tests`, including nested test modules.
- Ran repository-wide searches for ownership (`Rc`, `Arc`, `RefCell`, `Cell`), cloning/allocation, TODOs, fact payload consumers, wrapper arguments, function boundaries, and repeated helper shapes.
- Manually reviewed the lowering, name/value resolution, scope collection/query, fact construction/stream, ordinary and constrained matching, local and cross-project flow, project linking/session, lint/report assembly, rule API/compiler, and integration-test fixture paths.
- No tests or builds were run because this was a read-only audit and no production behavior was changed.
