# Codebase Readability Audit

## Summary

This audit covers all Rust source in `glass-lint-core` and `glass-lint-project`, including unit and integration tests. The codebase has strong domain modeling in several areas—normalized project paths, dense semantic IDs, immutable local artifacts, deterministic collections, and fail-closed status—but its largest readability and memory costs cluster where phase boundaries are represented by mutable coordinators rather than by types.

The 39 findings below include 13 high-, 21 medium-, and 5 low-severity items. The highest-value clean breaks are to make the project session, lowering pipeline, and scope collection phase-typed; preserve canonical/admitted paths as proof-carrying values; let linked matcher projections borrow their source model; expose the already-compositional compiled query model instead of maintaining parallel public matcher families; and eliminate the second path interner used by function summaries. These changes would make the lint pipeline visibly progress through admission, scope construction, resolution, fact freezing, linking, projection, and reporting while removing many current `Arc`, `RefCell`, snapshot, and clone-to-release-borrow workarounds.

## Findings

### READ-001 — Make project-session phases explicit in the public API
- **Severity:** High
- **Category:** API
- **Location:** `glass-lint-core/src/project/session.rs:358-695`

`AnalysisSession` exposes a runtime protocol—admit, analyze, record authored resolutions, then finish—through methods that all remain callable in invalid orders, so the implementation keeps parallel source, resolution, and artifact tables and revalidates identities at each step. A clean break into consuming types such as `AdmittingSession -> LocallyAnalyzedProject -> ResolvedProject` would make illegal sequences unrepresentable, let `admit_source` return a typed handle for `analyze`, and remove the string normalization/reconstruction in `add_source`.

### READ-002 — Preserve filesystem admission as a proof-carrying path
- **Severity:** High
- **Category:** Newtype
- **Location:** `glass-lint-project/src/admission.rs:26-150`, `glass-lint-project/src/walk.rs:53-104`, `glass-lint-project/src/discovery.rs:158-195`, `glass-lint-project/src/loader.rs:349-369`

Canonicalization and admission are repeatedly forgotten and recomputed: traversal classifies entries, discovery canonicalizes accepted paths again, and loading calls `admitted_path` on paths already returned by discovery or resolution. Distinct `CanonicalProjectPath` and `AdmittedSourcePath` types should carry containment/support guarantees into later phases, allowing discovery, queues, the resolver, and source loading to borrow one canonical allocation without repeated filesystem calls or ambiguous raw `PathBuf`s.

### READ-003 — Replace lowering-wide interior mutability with phase outputs
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/lowering.rs:116-266`, `glass-lint-core/src/analysis/name.rs:37-90`, `glass-lint-core/src/analysis/resolution/mod.rs:115-173`

The lowering entry point claims parsing, scope collection, fact construction, and effect extraction happen “in one pass,” but it performs several traversals and coordinates them through `RefCell<NameTable>`, `RefCell<ValueTable>`, and `RefCell<ResolverCache>`. Explicit `ScopedProgram`, mutable `ResolutionContext`, frozen `SemanticFacts`, and `LocalArtifact` transitions would document the real pipeline and permit ordinary exclusive/immutable borrowing instead of runtime borrow checks and final `into_inner` handoffs.

### READ-004 — Split predeclaration from source-order scope collection
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/collect/mod.rs:44-92`, `glass-lint-core/src/analysis/scope/collect/mod.rs:280-309`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:31-237`

`LexicalScopeCollector` is two visitors selected by a mutable `Pass` flag while also owning live assignment history, callbacks, aliases, dynamic-eval state, and structural synchronization diagnostics. A `ScopePlan` produced by a dedicated predeclaration visitor and consumed by a separate fact collector would remove pass-condition branches and `scope_diverged` mode management, and make the scope boundary visible to the rest of lowering.

### READ-005 — Unify the two parent-linked path interners
- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/value/path.rs:14-228`, `glass-lint-core/src/analysis/flow/summary.rs:25-225`

`PathInterner` and `SummaryPathInterner` independently implement bounded IDs, parent/depth nodes, edge interning, prefix checks, first-segment removal, rebuilding, and owned segment extraction. Generalize the canonical interner or add a summary overlay that references frozen `PathId`s and allocates only novel joined paths; this removes duplicated algorithms and avoids copying every referenced fact path into a second trie.

### READ-006 — Stop materializing linked copies of occurrence buckets
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/matching/mod.rs:305-395`, `glass-lint-core/src/analysis/project/projection.rs:54-82`

For every module, linking walks five occurrence families, clones identity keys, copies each occurrence into an overlay, normalizes the copied buckets, and then runs matching. A linked occurrence view or compact identity-key remap should borrow base buckets and translate query keys at lookup time, making the link output an identity mapping rather than a second occurrence index.

### READ-007 — Let matcher projection borrow the linked project
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:33-81`, `glass-lint-core/src/analysis/project/projection.rs:21-33`, `glass-lint-core/src/analysis/project/projection.rs:49-105`

`SemanticFacts` wraps its index in an `Arc` even though its enclosing semantic artifact is already shared, and `ProjectMatcherModel` clones one `Arc` per module despite being created from and consumed alongside a borrowed `ProjectSemanticModel`. Give `ProjectMatcherModel` a project lifetime and store `&OccurrenceIndexes`; this makes the projection phase's dependency explicit and removes nested allocations and atomic reference-count traffic.

### READ-008 — Separate flow evidence mutation from state inspection
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/evidence.rs:81-180`

Sink and helper projection clone `FlowId` slices, `FlowState`s, `CompiledObjectFlow`s, and whole helper summaries into temporary vectors solely so `&mut self` can emit evidence after immutable lookups. Split the projector into independently borrowed state/index/evidence fields, or make emission a helper that mutates only `FlowEvidence`, so ready states and matchers can remain borrowed throughout the hot path.

### READ-009 — Expose disjoint summary-table access instead of cloning calls
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:365-461`, `glass-lint-core/src/analysis/flow/table.rs:21-69`

Direct-sink and fixed-point propagation repeatedly clone each function's call list, collect function IDs, linearly search prior offsets, and materialize projected sinks before reacquiring a mutable caller. Extend `FunctionTable` with dense indexed snapshots and safe disjoint access (or compute one indexed delta round), allowing borrowed call slices and direct target/caller access without per-round clone-to-release-borrow staging.

### READ-010 — Compile matcher declarations into one compositional public model
- **Severity:** High
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/matcher/call.rs:8-149`, `glass-lint-core/src/api/rule/matcher/member.rs:11-168`, `glass-lint-core/src/api/rule/matcher/derived.rs:5-167`, `glass-lint-core/src/api/compiler/rule.rs:36-139`

Call, member-call, constructor, class, and member-read types repeat identity constructors, evidence formatting, sort keys, normalization, and argument convenience methods, while the compiler immediately lowers them into the more coherent `IdentityConstraint + EventPredicate + SubjectConstraint + constraints` model. Replace the parallel public families with a validated compositional pattern builder around those dimensions; a clean break avoids another wrapper layer and gives argument constraints one owner.

### READ-011 — Compile and retain one catalog representation
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/catalog.rs:34-136`, `glass-lint-core/src/api/compiler/rule.rs:250-333`

`RuleCatalog` retains full normalized matcher declarations alongside compiled plans; compilation first copies `rule.matchers()` into a `MatcherSet`, clones it again for normalization, and `combine` discards incoming compiled catalogs and recompiles every rule. Make catalog construction consume rule definitions into a single compiled rule record containing metadata plus query plan, so combining catalogs moves already-compiled entries and matcher declaration trees do not remain resident.

### READ-012 — Compile tsconfig patterns once per configuration
- **Severity:** High
- **Category:** Newtype
- **Location:** `glass-lint-project/src/discovery.rs:173-195`, `glass-lint-project/src/discovery.rs:269-283`

Every discovered path recompiles every include and exclude glob, while `tsconfig_pattern_matches` also allocates normalized pattern strings for each comparison. A validated `TsconfigPatternSet` should normalize and compile patterns once when the effective config is built, then provide allocation-free borrowed matching during discovery.

### READ-013 — Keep one canonical project-input admission algorithm
- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/input.rs:34-142`

`ProjectInput::validate` and `validate_into_maps` independently normalize sources and resolutions, check duplicates/importers, and build the same maps; comments explicitly acknowledge the map-to-vector-to-map path. Make consuming validation produce one public validated, map-backed project type, with an explicit DTO conversion only for callers that truly need vectors.

### READ-014 — Do not erase the validated-options boundary with `Deref`
- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-project/src/options.rs:49-92`, `glass-lint-project/src/loader.rs:139-148`, `glass-lint-project/src/corpus.rs:72-88`

`ValidatedProjectLoadOptions` dereferences to the raw type, `ProjectLoader::options` returns the raw type, `SourceAdmission` accepts raw options, and `SourceCorpus` offers both checked and unchecked constructors. Keep validated fields private behind explicit accessors and require the validated policy at every I/O boundary, eliminating repeated validation and caller-visible “unchecked” escape hatches.

### READ-015 — Normalize extension policy during validation
- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-project/src/options.rs:190-223`, `glass-lint-project/src/options.rs:225-291`

`supports` lowercases the entire path and every configured extension on every query even though extensions have already passed a validation boundary. Store canonical suffixes in a domain collection such as `SourceExtensionSet`, centralize declaration-file exclusions there, and let admission perform one lowercase conversion at most.

### READ-016 — Model tsconfig loading with typed intermediate data
- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/discovery.rs:105-156`, `glass-lint-project/src/discovery.rs:199-259`, `glass-lint-project/src/discovery.rs:309-325`

Discovery passes mutable `serde_json::Value`s through free functions that interpret string keys, recursively merge only selected subtrees, and share a visitation set across inheritance and references. A small `Tsconfig`, `TsconfigChain`, and `SourceSelection` model would give parsing/merging/reference traversal distinct phases, centralize supported semantics, and make unsupported or malformed fields explicit instead of silently disappearing through `Value` queries.

### READ-017 — Remove the resolution cache's ineffective `Arc`
- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:250-260`, `glass-lint-project/src/loader.rs:389-410`

`ResolutionCache` allocates an `Arc<ResolverOutcome>` and clones the handle, but `record_resolution` then deep-clones the outcome into the core session, so the reference counting does not avoid the owned copy. Store outcomes once and finalize/move the cache into the session after queue expansion, or record stable outcome IDs during traversal and consume the table at the phase boundary.

### READ-018 — Record exclusive phase timings and derive aggregates
- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:73-100`, `glass-lint-project/src/loader.rs:447-460`

`linking`, `matching`, and `linking_and_matching` overlap, while `parse_and_local_analysis` is a composite field beside otherwise narrower phases. Store exclusive phase durations in a typed timeline and derive composites for reporting; this prevents accidental double counting and makes the loader/core pipeline boundary apparent.

### READ-019 — Return failure when the current directory is unavailable
- **Severity:** Medium
- **Category:** Other
- **Location:** `glass-lint-project/src/admission.rs:162-168`

`absolute_path` turns a `current_dir` failure into an empty default path, silently changing path interpretation at the project boundary. Return `Result<AbsoluteProjectPath, ProjectLoadError>` and preserve the I/O cause so project selection never proceeds from a fabricated base directory.

### READ-020 — Replace catch-all option messages with domain errors
- **Severity:** Medium
- **Category:** API
- **Location:** `glass-lint-project/src/error.rs:43-70`, `glass-lint-project/src/corpus.rs:103-119`, `glass-lint-project/src/discovery.rs:262-267`, `glass-lint-project/src/loader.rs:45-55`

`ProjectOptionError::Message(String)` represents a non-directory corpus root, tsconfig parse failures, and a core partial-load reason, conflating configuration, selection, parsing, and analysis outcomes and discarding structured context. Add dedicated variants (including config path and source error where available), and keep partial analysis reasons out of the options error hierarchy.

### READ-021 — Centralize constant provenance and member evaluation
- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/constants.rs:14-79`, `glass-lint-core/src/analysis/scope/query/constants.rs:19-80`, `glass-lint-core/src/analysis/resolution/mod.rs:133-167`

The collector and frozen scope graph duplicate the full `BindingProvenance -> ConstValue` conversion, and all three `Lookup` adapters duplicate static array/object member evaluation. Put provenance conversion behind one helper accepting a name resolver and provide the shared member behavior as a `Lookup` default/helper, leaving each adapter responsible only for its distinct shadowing and mutability policy.

### READ-022 — Share one destructuring projection walker
- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/aliases.rs:17-116`

Declaration aliases and assignment aliases recursively walk the same `Pat::Ident`/object key-value/object shorthand structure, differing only in whether the resulting binding is inserted or recorded at a span. Extract a conservative pattern projection iterator/visitor that yields `(binding, projected_path)` and give the two callers their own sinks; this also centralizes the policy for rest/default/dynamic forms and name exhaustion.

### READ-023 — Give `FactStream` sole authority over fact IDs and budgets
- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/facts/build/mod.rs:78-145`, `glass-lint-core/src/analysis/facts/build/mod.rs:180-210`, `glass-lint-core/src/analysis/facts/stream.rs:23-71`

`FactBuilder` allocates IDs and enforces a configurable limit while `FactStream::push` separately checks the global limit, dense ordering, and validity. A bounded `try_push(span, payload) -> Result<FactId, FactIssue>` on the append-only stream would make dense identity and exhaustion one invariant and remove the builder's parallel `next_id` state.

### READ-024 — Use one linked-identity domain type
- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/model.rs:55-73`, `glass-lint-core/src/analysis/matching/mod.rs:127-166`, `glass-lint-core/src/analysis/project/model.rs:498-509`

`ExportResolution` and `LinkedModuleIdentity` mirror external, global, qualified, static-string, and unknown states, but differ in ID representation and collapse `Ambiguous` during an owned conversion. Keep one authoritative linked identity (or a borrowed matcher view of it) so new variants and ambiguity policy cannot drift and projection does not rebuild equivalent maps.

### READ-025 — Factor event-index selection out of query execution
- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/query.rs:169-336`

`occurrences_for_event` repeats exact/package module lookup, overlay masking, and base/overlay selection across calls, member calls, member reads, classes, and constructions; the member call/read arms are almost identical. Introduce typed event-index views whose only variation is the relevant base/overlay family, then centralize identity dispatch and package/exact lookup once.

### READ-026 — Encapsulate occurrence-index family maintenance
- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/matching/build.rs:19-44`, `glass-lint-core/src/analysis/matching/mod.rs:216-235`, `glass-lint-core/src/analysis/matching/mod.rs:343-365`

Normalization, emptiness checks, and overlay normalization manually enumerate many individual indexes, so adding an index requires updating several distant lists. Give call, member, construction, and literal index families their own `normalize`/`is_empty` behavior and have `OccurrenceIndexes` delegate to those cohesive owners.

### READ-027 — Avoid whole-table copy-on-write for control-flow snapshots
- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:61-112`, `glass-lint-core/src/analysis/flow/projector/state.rs:172-184`, `glass-lint-core/src/analysis/flow/projector/state.rs:276-327`

`Arc<Vec<_>>` makes capture cheap, but the first alias/state mutation on each branch clones the entire sorted vector and insert/remove then shifts entries; joins allocate complete replacement tables. A checkpoint/rollback mutation log or persistent delta map would make branch state explicit while copying only changed bindings and requirements, which is especially valuable for deeply nested control flow.

### READ-028 — Give call arguments a stable effect-call identity
- **Severity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:51-92`, `glass-lint-core/src/analysis/flow/effect.rs:515-526`, `glass-lint-core/src/analysis/flow/effect.rs:545-578`

Every `EffectUse::CallArgument` stores a fact event plus index, and consumers linearly search `FunctionEffect.calls` by event to recover its argument. Allocate a dense `EffectCallId` when recording the call and have uses point directly to that slot; the call remains the single owner of event identity and arguments.

### READ-029 — Return a borrowed-or-owned call chain
- **Severity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:189-228`, `glass-lint-core/src/analysis/flow/projector/mod.rs:215-224`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:20-31`

`CallEffectRef::chain_owned` clones an existing interned `NamePath` for the common case because only the callee-name fallback requires construction. Return `Cow<'_, NamePath>` or a small `ResolvedNamePathRef` enum so direct/rooted/syntactic chains remain borrowed in local and cross-flow loops.

### READ-030 — Avoid cloning local statuses before propagation
- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/model.rs:278-295`

`propagate_local_status` clones every module's complete `AnalysisStatus` into a temporary vector solely to mutate the project's status field. Split borrows of `modules` and `status`, or add an extension method that accepts borrowed module/status pairs, so status entries are cloned only when they become owned project entries.

### READ-031 — Store related evidence once per rule result
- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/linter.rs:338-380`

Project reporting groups related evidence by rule and then clones the entire related list into every finding for that rule, producing `findings × related` owned evidence. Represent rule-level related evidence once (or share an immutable evidence slice/`Arc<[Evidence]>`) and let findings reference it during serialization/report assembly.

### READ-032 — Use a compact indexed artifact-cache key
- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/local.rs:46-100`, `glass-lint-core/src/analysis/local.rs:136-201`

Each cache lookup holds a mutex while linearly comparing up to 64 keys containing full `SourceText`, `Environment`, and `AnalysisLimits`, and FIFO eviction shifts the vector with `remove(0)`. Use a precomputed semantic fingerprint/index outside the lock plus a map and queue (retaining collision verification if required by strict identity), so concurrent local analysis does not serialize on repeated full-source comparisons.

### READ-033 — Make the local executor consume an iterator and return errors
- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/session.rs:20-29`, `glass-lint-core/src/project/session.rs:159-220`, `glass-lint-core/src/project/session.rs:541-607`

Batching repeatedly drains from the front of `Vec`s into new vectors, and the executor trait cannot return failure, so worker panics become `expect("analysis worker panicked")` while the caller carries an otherwise unnecessary `Result`. A result-returning executor over an owned iterator/queue would remove shifting and intermediate batches, make thread failure explicit, and simplify the session's `unnecessary_wraps` exception.

### READ-034 — Make analysis limits valid by construction
- **Severity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/limits.rs:5-82`

All six public raw `usize` fields admit zero and require repetitive validation that returns unstructured `String` errors. Private `NonZeroUsize`/domain budget fields with a typed `AnalysisLimitError` would remove invalid post-construction states and let downstream phases borrow trusted limits without defensive validation.

### READ-035 — Build each project file report once
- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lint/linter.rs:383-417`

`initialize_project_files` first creates an empty `FileReport` for every source, then reconstructs and replaces the same record for parse failures while separately cloning a `parse_paths` list. Consume diagnostics while creating the initial map and derive status input from that typed parse-failure collection instead of maintaining parallel representations.

### READ-036 — Share the `SourceLineIndex` constructor body
- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/diagnostic.rs:251-277`

`new` and `from_text` duplicate line-start and checkpoint construction, differing only in whether source text is newly allocated or already shared. Convert to `SourceText` first and delegate both public constructors to one private implementation.

### READ-037 — Do not clone owned values merely to sort
- **Severity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/query.rs:100-103`, `glass-lint-core/src/lint/linter.rs:318-325`

Evidence sorting clones each symbol into the sort key and finding sorting clones each `RuleId`, even though both values can be compared by reference. Use `sort_by` with borrowed tuple components (or a genuinely cached compact key) to keep deterministic ordering without transient ownership churn.

### READ-038 — Remove the redundant selected-rule lookup
- **Severity:** Low
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/model.rs:445-464`

Classification calls `rules.get(index)` to test for `None` and immediately performs the identical lookup in `let Some(rule)`. Keep only the binding lookup so the control flow expresses the actual guard once.

### READ-039 — Stop leaking name tables in test helpers
- **Severity:** Low
- **Category:** Testing
- **Location:** `glass-lint-core/src/analysis/name.rs:80-87`, `glass-lint-core/src/analysis/resolution/mod.rs:214-227`

Test constructors use `Box::leak` to manufacture `'static` name-table contexts, obscuring the ownership contract and accumulating allocations across the test process. Use a scoped fixture/callback that owns the `RefCell<NameTable>` for the test's duration; this also exercises the same borrowing shape a phase-oriented production design would use.

## Systemic Themes

- **Phase state is often implicit.** `AnalysisSession`, lowering, scope collection, linked projection, and report assembly all store several lifecycle stages in one mutable owner. Consuming phase types would make the pipeline readable from signatures and remove runtime ordering checks.
- **Clone-to-release-borrow is a symptom, not the root cause.** Flow projection, summary propagation, status propagation, linked occurrence overlays, and report assembly clone data because broad `&mut self` methods prevent disjoint field borrowing or because intermediate models own data they could view.
- **Validated values lose their proof too early.** Filesystem paths revert to `PathBuf`, validated options dereference to raw options, fact allocation is split between builder and stream, and linked identity is converted into a second enum. Proof-carrying newtypes should cross phase boundaries intact.
- **Public matcher declarations and internal queries are parallel models.** The internal query dimensions are already the more compositional abstraction; exposing a validated version of them would remove much of the public API duplication and let catalogs retain only compiled plans.
- **Repeated normalization hides avoidable work.** Extension lowercasing, glob compilation, path canonicalization, occurrence normalization, matcher normalization, and full cache-key comparison are performed after the relevant admission/compile boundary instead of being captured by it.

## Open Questions

- Must callers serialize a validated `ProjectInput` back into the current vector DTO shape, or can the clean-break API expose a distinct map-backed `ValidatedProject` and keep the wire DTO explicitly unvalidated?
- Is `ProjectMatcherModel` intended to outlive `ProjectSemanticModel`? Current use appears phase-local; if that is an undocumented requirement, it explains the `Arc`s but should be made explicit before converting the model to borrowed views.
- Are overlapping timing fields consumed as independent metrics by downstream harnesses? If so, can the harness derive them from exclusive timestamps rather than requiring the loader to store overlapping durations?
- Does strict cache identity permit a cryptographic content fingerprint with collision verification on matching buckets, or must full source equality remain the primary key operation?
- Should tsconfig inheritance/reference cycles produce a diagnostic rather than silently terminate through `visited`? A typed config-chain phase would make that policy explicit.
- Are normalized public matcher declarations needed after catalog construction for introspection? If they are, expose a deliberate compiled-plan inspection view rather than retaining both complete declaration and execution trees by default.

## Coverage

- Reviewed all 135 Rust files in `glass-lint-core` (38,288 lines), including parsing, configuration, public rule/matcher APIs, compilation, scope/provenance, resolution, semantic facts, value/name arenas, local and cross-module flow, project linking/projection, reporting, cache/session orchestration, and all unit/integration test modules.
- Reviewed all 10 Rust files in `glass-lint-project` (2,136 lines), including options, admission, walking, corpus loading, tsconfig discovery, resolver configuration/classification, loader orchestration, errors, and tests.
- Read the repository and owning-crate architecture documents plus `TESTING.md` and `CONTRIBUTING.md` before evaluating boundaries and public APIs.
- This was a static readability/maintainability audit. No source code, tests, configuration, or documentation other than this report was changed, and runtime benchmarks were not used to rank memory-churn findings.
