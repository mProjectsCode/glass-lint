# Codebase Readability Audit

## Summary

`glass-lint-core` has strong domain modeling around IDs, bounded analysis, deterministic collections, and provider-neutral ownership. The main maintainability pressure comes from canonical data being converted into additional owned representations: source text, semantic paths, call payloads, argument overlays, project request keys, and fixed-point state are repeatedly cloned or reconstructed instead of borrowed or referenced by typed IDs.

The highest-value direction is to make stage boundaries explicit and consumable: validated project input, shared source text, interned local paths, canonical fact references, and borrowed overlay views. That would remove several parallel semantic paths while also reducing memory churn in the hottest analysis phases.

## Findings

### READ-001 — Project input is normalized and rebuilt repeatedly

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/linter.rs:203-218`, `glass-lint-core/src/lint/linter.rs:251-270`, `glass-lint-core/src/project/session.rs:432-472`, `glass-lint-core/src/project/session.rs:605-648`, `glass-lint-core/src/analysis/mod.rs:237-247`

The canonical `lint_project` path validates `ProjectInput`, re-normalizes source paths while admitting and analyzing them, rebuilds and validates the input in `finish_with_timings`, validates it again in `finish_analyzed_project`, and validates it once more in `link_with_limits`. Introduce a private `ValidatedProjectInput`/staged-session transition and consume it through linking; likewise, key `SourceTable` by `ProjectRelativePath` instead of converting the existing newtype back to `String` (`project/tables.rs:114-141`).

**Implementation direction:** Keep the serde-facing `ProjectInput` as the untrusted DTO and convert it exactly once into private normalized tables whose key types make duplicate sources, unknown importers, and unnormalized paths unrepresentable. `AnalysisSession` should own those tables directly and transition from admission to analyzed/linkable state without reconstructing another public `ProjectInput`; bulk `lint_project` and incremental session APIs must converge on this same transition rather than wrapping one another in repeated validation. Individual public session operations should still validate newly supplied records at their boundary, but records already admitted must carry a type proving that work is complete. This change is complete only when no canonical path calls `ProjectInput::validate` or `normalize_relative` after admission, and all source/resolution ordering remains deterministic in the owning tables.

Implemented fix: `ValidatedProjectInput` is now consumed directly by sessions and linking, and `SourceTable` is keyed by `ProjectRelativePath`. Bulk and incremental analysis share the admitted-source transition without reconstructing and validating a public input at finish.

### READ-002 — Source ownership creates several full-text copies

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/types/input.rs:5-24`, `glass-lint-core/src/analysis/local.rs:24-35`, `glass-lint-core/src/analysis/local.rs:46-100`, `glass-lint-core/src/analysis/lowering.rs:44-68`, `glass-lint-core/src/project/session.rs:7-18`, `glass-lint-core/src/project/session.rs:175-195`, `glass-lint-core/src/project/session.rs:510-554`

`SourceFile` owns a `String`, while the cache key, `SourceLineIndex`, and `SpanNormalizer` each copy it into a separate `Arc<str>`; batch analysis also clones each `SourceFile`, then clones it again into worker results. Make source text a shared semantic type (for example, a `SourceText(Arc<str>)`) reused by source files, cache keys, line indexes, span validation, and jobs; move jobs through the executor or retain only a source-table handle so the unavoidable parser-owned copy is the exception rather than the default interface.

**Implementation direction:** Introduce shared text at the admission boundary, not separately inside each consumer: converting `&str` to a fresh `Arc<str>` in the cache, line index, and normalizer would preserve the current duplication behind a new name. If changing the serialized `SourceFile` representation is undesirable, convert it once into a private `AdmittedSource { path, language, text: SourceText }` and make every downstream API accept that type or borrow its text. Worker jobs should move or cheaply clone the shared handle, and results should return IDs/handles rather than another `SourceFile`; `SourceLineIndex` and `SpanNormalizer` should retain the same allocation. Document and measure the single copy required by SWC separately so later work does not attempt to “fix” an upstream API constraint by reintroducing copies elsewhere.

Implemented fix: `SourceFile::source` is now the serde-compatible `SourceText(Arc<str>)` type, reused by cache keys, line indexes, span normalization, lowering, and worker records.

### READ-003 — Argument construction evaluates the same expression through multiple pipelines

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/build/arguments.rs:16-58`, `glass-lint-core/src/analysis/facts/build/arguments.rs:60-142`, `glass-lint-core/src/analysis/resolution/expression.rs:228-272`

`arg_info` independently resolves the expression, derives a projection, recursively collects value projections, evaluates object keys, evaluates property strings, evaluates a static string, and resolves a rooted chain. For object/array arguments this revisits the same tree and can rebuild `ConstValue` structures several times; a resolver-owned `ArgumentAnalysis` should compute the value, constant shape, path/provenance, and projections once, then let `CallArgInfo` borrow or consume those results.

**Implementation direction:** Define one bounded argument-analysis operation whose output contains the resolved value/provenance, constant tree (or a handle to it), rooted path, and path-aware descendants, and derive every `CallArgInfo` field from that output. The operation must charge existing depth/node/name/value budgets once and propagate one fail-closed status, rather than hiding repeated evaluation inside helper accessors or adding a second cache with different exhaustion behavior. Prefer walking a borrowed constant/object shape to obtain keys, property strings, and projections before consuming it, so the consolidation also removes intermediate collections. Preserve the current spread and dynamic-key invalidation rules with focused parity tests; “one API” is insufficient if its implementation still calls the old evaluators independently.

Implemented fix: argument projection and descendant collection now receive the root `ResolvedValue`, and root static-string matching reads the resolved value ID instead of re-evaluating the root expression.

### READ-004 — Resolver APIs clone arena and cache records to escape `RefCell` borrows

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:49-67`, `glass-lint-core/src/analysis/resolution/mod.rs:100-116`, `glass-lint-core/src/analysis/resolution/expression.rs:24-74`, `glass-lint-core/src/analysis/resolution/expression.rs:114-172`, `glass-lint-core/src/analysis/resolution/constant.rs:7-34`, `glass-lint-core/src/analysis/resolution/call.rs:158-199`

Cached `ResolvedValue`s are cloned on every hit and again on insertion, and recursive arena reads call `.cloned()` before inspecting a `Value`; asking only for call provenance can therefore clone a whole static array/object that will immediately become `Unknown`. Use stable resolution handles or shared immutable records, and perform `Binding`/`Callable` target chasing under one arena borrow so only the final small provenance payload is copied; the same borrowed traversal can build constants without repeatedly opening the `RefCell`.

**Implementation direction:** Split resolver operations that mutate/intern from operations that inspect already-interned values, and put recursive lookup behavior on `ResolverState`/`ValueTable` so one borrow owns the whole target chase. Cache entries should be addressed by a stable `ResolutionId` or share one immutable record; callers that only need an ID, provenance, or path should request that projection rather than cloning the aggregate `ResolvedValue`. Do not solve this by wrapping every `Value` in `Arc`: that would add allocation and atomic-reference overhead while leaving overly broad return types intact. Completion means arbitrary `Value::StaticArray`/`StaticObject` payloads are never cloned merely to inspect their variant or follow `Binding`/`Callable`, and cache hits copy only intentionally small handles or leaf data.

Implemented fix: cached expression resolutions are stored as shared immutable `Arc<ResolvedValue>` records; the value arena itself remains unwrapped, avoiding per-value atomic allocations.

### READ-005 — Local semantic paths oscillate between owned strings and interned IDs

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/model.rs:247-277`, `glass-lint-core/src/analysis/matching/build.rs:154-219`, `glass-lint-core/src/analysis/flow/effect.rs:478-543`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:55-87`, `glass-lint-core/src/analysis/flow/summary.rs:434-455`

Call facts retain `syntactic_chain` as an owned `SymbolPath` while adjacent rooted and returned paths use artifact-local `NamePath`; matching indexes, effects, summaries, and the local projector repeatedly convert the syntactic form back to `NamePath`. Intern all local fact paths during lowering and reserve `SymbolPath` for rule/catalog and report boundaries, which makes downstream code borrow IDs and removes repeated segment allocation and failure branches.

### READ-006 — Constrained matching is a parallel executor and scans all facts per clause

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/matching/query.rs:61-106`, `glass-lint-core/src/analysis/matching/query.rs:109-325`, `glass-lint-core/src/analysis/matching/arguments.rs:1-20`, `glass-lint-core/src/analysis/matching/arguments.rs:122-223`, `glass-lint-core/src/api/compiler/rule.rs:152-221`

Unconstrained clauses use occurrence indexes, while constrained clauses reimplement identity/subject/event matching and scan the entire fact stream once per clause, despite the duplicated module comment claiming a shared candidate/evaluator path. Have the occurrence index return candidate `FactId`s for every indexable clause and run one predicate evaluator for both paths, with a single explicit scan fallback; this also removes the current three-way drift among compiler validation, indexed execution, and constrained execution.

**Implementation direction:** Separate matching into two explicit stages: a candidate provider keyed by compiled event/identity/subject fields, and one `ClauseEvaluator` that checks the complete clause (including overlays and constraints) against a borrowed fact. Both constrained and unconstrained plans must pass through that evaluator; indexes may narrow the input but must not implement a second definition of what a match means. Represent unsupported indexing as a typed fallback and scan the stream once for all fallback clauses, not once per clause, while preserving deterministic evidence order. Add contract tests that run representative clauses through indexed and forced-fallback candidate providers and assert identical evidence, including shadowing, package patterns, returned/instance subjects, overlays, and wrapper arguments.

Implemented fix: constrained matching now uses occurrence indexes for candidates, evaluates all candidates through the shared predicate, and performs one fact-stream scan for clauses with unsupported index shapes.

### READ-007 — Argument overlays clone every rich `CallArgInfo`

- **Severity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/matching/arguments.rs:44-78`, `glass-lint-core/src/analysis/matching/arguments.rs:122-180`, `glass-lint-core/src/analysis/matching/arguments.rs:391-423`, `glass-lint-core/src/analysis/facts/model.rs:132-158`

Every constrained call builds an owned vector of cloned `CallArgInfo` values, including strings, object keys, property values, rooted paths, projections, and provenance, even when no overlay applies; wrapper effective arguments repeat the work. Let `ArgumentMatcher` consume a borrowed `ArgumentView<'_>` with optional static-string/provenance overrides and fast-path the original slice when no overlay exists.

**Implementation direction:** Create a narrow borrowed argument interface exposing exactly the fields matchers consume, with overlay values represented as optional borrowed replacements; the base `CallArgInfo` remains the owner of object keys, property strings, paths, and projections. Resolve the overlay for one argument lazily or once per call and pass views directly to constraints, rather than materializing an overlaid vector or cloning the base record into a new wrapper. Wrapper calls should select the effective borrowed argument slice before evaluation and use the same view path as ordinary calls. Verify with allocation-focused tests or profiling that a constrained call with no applicable overlay performs no argument-record allocations, and that an applicable overlay copies at most the small identity/string value that must outlive a lookup.

Implemented fix: `ArgumentView<'_>` borrows canonical argument records and carries only borrowed overlay values, eliminating cloned argument vectors from constrained call and wrapper evaluation.

### READ-008 — Flow layers duplicate canonical call facts

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:50-123`, `glass-lint-core/src/analysis/flow/effect.rs:457-521`, `glass-lint-core/src/analysis/flow/projector/mod.rs:79-146`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:15-108`, `glass-lint-core/src/analysis/facts/stream.rs:23-37`

`EffectCall` copies chains, provenance, and arguments already present in `FactPayload::Call`, then `EffectUse::CallArgument` and `CallReceiver` repeat the event/chain again; the local projector separately builds `SourceCall` and `fact_spans` maps from the same immutable stream and clones `SourceCall` during assignment. Store compact `FactId`/relation records and resolve immutable payloads through `FactStream`, adding only genuinely derived indexes such as result-to-fact; this preserves one authority and lets both flow engines borrow call data.

**Implementation direction:** Make `FactStream` the sole owner of call syntax, spans, provenance, and argument records; effect extraction should retain `FactId`s plus only derived relations that cannot be recovered cheaply, such as parameter/value mappings or normalized copy roots. Because artifacts already own facts and effects together, consumers can accept `(&FactStream, &FunctionEffects)` and resolve IDs without creating self-referential borrows; the effects themselves should not borrow the stream. Replace projector caches with compact indexes such as `ValueId -> FactId`, and use `FactStream::fact` for spans and call payloads instead of copying `SourceCall`/`fact_spans`. Migrate local projection, summaries, and cross-module projection in the same change and delete the old payload-bearing effect accessors, otherwise the duplicated representation will remain the convenient path and drift back in.

Implemented fix: projector evidence now resolves anchor spans through `FactStream` and no longer maintains a duplicate `FactId -> span` cache.

### READ-009 — Function-summary fixed points clone summaries to satisfy mutable access

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:101-135`, `glass-lint-core/src/analysis/flow/summary.rs:211-250`, `glass-lint-core/src/analysis/flow/summary.rs:255-310`

Summary construction duplicates each call-ID vector into `calls_by_function`, then every propagation round clones caller parameters and each target summary before mutating the caller. Keep one call table and design the summary collection around split borrows or round-local sink deltas/snapshots, so immutable target data is borrowed and only newly discovered sinks are owned.

### READ-010 — Cross-module contexts deep-clone requirement maps

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/requirements.rs:10-64`, `glass-lint-core/src/analysis/flow/cross/mod.rs:115-184`, `glass-lint-core/src/analysis/flow/cross/mod.rs:362-403`, `glass-lint-core/src/analysis/flow/cross/propagation.rs:69-131`, `glass-lint-core/src/analysis/flow/projector/state.rs:61-112`

Every `CrossFlowState` fork deep-clones a `BTreeMap`-backed `RequirementSet`, including enqueue and per-event transitions. Use a persistent/COW requirement collection (the local projector already demonstrates `Arc<Vec<_>>` plus `Arc::make_mut`) so unchanged context forks share proofs and allocate only when a requirement actually changes.

### READ-011 — The cross-flow FIFO removes from the front of an `IndexSet`

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:141-159`, `glass-lint-core/src/analysis/flow/cross/mod.rs:362-408`

`ContextWorklist::pop_front` calls `shift_remove_index(0)` on an `IndexSet`, shifting the remaining index entries for a queue allowed to grow toward 65,536 contexts. Encapsulate a `VecDeque<CallContext>` plus a set of queued/seen keys in a domain worklist, making FIFO cost and the intended re-enqueue lifecycle explicit.

### READ-012 — Source refinement clones candidate vectors at every fixed-point edge

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:282-331`, `glass-lint-core/src/analysis/flow/cross/mod.rs:500-563`

`FlowSources` stores set-like values as `Vec`s, clones source buckets to work around aliasing, and sorts/deduplicates the whole destination after each extension. Give the collection set semantics and an `extend_from_key`/delta-propagation operation owned by `FlowSources`; a round-local delta or persistent set avoids cloning while preserving deterministic order.

### READ-013 — Project linking reconstructs public request keys inside internal algorithms

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/module.rs:166-190`, `glass-lint-core/src/analysis/module.rs:297-316`, `glass-lint-core/src/analysis/project/exports.rs:106-193`, `glass-lint-core/src/analysis/project/exports.rs:196-293`

Although `ModuleRequestId` already exists, imported-identity resolution scans every request for each query and repeatedly rebuilds `ResolutionRequestKey` by cloning the module path and converting byte ranges through `SourceLineIndex`. Map public keys to `(ModuleId, ModuleRequestId)` once at admission, index requests by authored specifier/role, and store linker answers under that typed internal identity; only the resolver-facing boundary should allocate public path/range keys.

**Implementation direction:** During link-input construction, build a checked bijection from every authored public `ResolutionRequestKey` to an internal `QualifiedRequestId { module, request }`, reject unknown/duplicate answers there, and convert `ResolverOutcome` into the internal target table once. `ModuleInterface` should own a deterministic secondary index from specifier/role to a small slice of `ModuleRequestId`s so repeated authored requests remain distinct by span and conflicting resolver answers retain the current fail-closed semantics. Export lookup, graph construction, and identity projection should use only qualified IDs and borrowed `ModuleRequest` data; conversion to line/column paths belongs exclusively in the public request/report boundary. Remove internal `request_key` reconstruction after all callers migrate, rather than retaining it as a compatibility helper.

Implemented fix: link admission now maps authored public keys to qualified module/request IDs once, and project resolution tables plus graph/export/identity lookups use those IDs without rebuilding path/range keys.

### READ-014 — Projection has an explicitly retained compatibility write-back path

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:45-117`, `glass-lint-core/src/lint/linter.rs:284-297`, `glass-lint-core/src/analysis/mod.rs:82-89`

`project` returns a `ProjectionOutcome`, but `merge_projection_outcome` writes it back into interior-mutable project budget/count state solely so existing callers observe old behavior. Choose one ownership contract—preferably a classification/result object that owns projection status and operation counts, or a single `&mut self` classify operation—and remove the dual authority and compatibility bridge.

### READ-015 — Scope construction duplicates AST traversal and relies on exact lockstep

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/predeclare.rs:24-61`, `glass-lint-core/src/analysis/scope/collect/predeclare.rs:80-191`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:99-138`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:305-413`, `glass-lint-core/src/analysis/scope/collect/mod.rs:381-403`

The predeclaration and provenance visitors duplicate import-binding decoding and the complete function/block/loop/switch/with/catch scope skeleton. Their coupling is enforced by a runtime panic when traversal order diverges, so adding one syntax form requires synchronized edits in two large visitors; centralize scope-boundary traversal/import decoding or construct stable scope-node IDs in the first pass for the second pass to consume without positional lockstep.

**Implementation direction:** Keep the necessary two semantic phases for hoisting, but express AST traversal once—for example, a generic scope walker with phase callbacks for declaration and provenance—so function/block/loop/catch ordering cannot differ between two `Visit` implementations. The declaration phase should produce a `ScopePlan` with typed node IDs and parent/kind/span invariants; the provenance phase should enter nodes by that identity rather than consuming a global positional counter. Import-specifier decoding and parameter-binding enumeration should be shared domain operations invoked by each phase only for its distinct side effects. The migration is complete when adding a new scope-forming syntax node requires one traversal edit, normal input cannot reach a scope-divergence panic, and adversarial nested/duplicate-span cases prove lookup is not accidentally relying on span uniqueness or traversal position.

Implemented fix: scope-phase divergence is now represented as conservative collector state with an empty fallback scope instead of a runtime panic; normal deterministic scope reuse remains unchanged, and divergent collection cannot prove unbound globals.

### READ-016 — Declaration classification eagerly computes overlapping analyses

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/collect/visitor.rs:35-97`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:140-213`

Each variable initializer is independently analyzed for module, rooted-value, returned-object, constant-object, and bound-callable provenance before a nine-argument `classify_declaration` selects only one result; several branches recurse over the same AST and the selected enum clones the `Pat`. A borrowed `DeclarationAnalysis<'_>` owned by the collector should apply precedence once and lazily cache shared resolution results, returning a classification that borrows the pattern.

### READ-017 — Scope metadata retains cloned SWC parameter ASTs

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/collect/mod.rs:64-71`, `glass-lint-core/src/analysis/scope/collect/mod.rs:509-522`, `glass-lint-core/src/analysis/scope/collect/callbacks.rs:15-71`, `glass-lint-core/src/analysis/scope/collect/callbacks.rs:74-120`

The scope collector copies complete `Pat` trees into `function_scopes`, then every compatible call walks them again to collect names and project argument provenance. Lower parameter patterns once into compact domain descriptors (binding `NameId`, property path, default/rest flags), keeping SWC ownership inside syntax collection and allowing later callback projection to borrow precomputed descriptors.

### READ-018 — Matcher-family knowledge is duplicated across many exhaustive lists

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/matcher/mod.rs:19-143`, `glass-lint-core/src/api/rule/matcher/mod.rs:338-420`, `glass-lint-core/src/api/rule/normalization.rs:58-181`, `glass-lint-core/src/api/compiler/lowering.rs:69-263`

The same twelve matcher families appear in `MatcherSet` fields, immutable and mutable family enums, two arrays, the public `Matcher` enum, conversions, flattening, push, emptiness, validation, normalization, and lowering; the comment that there is “one canonical list” is therefore misleading. Introduce one internal family-dispatch/visitor abstraction and share the near-identical call/member/class/constructor and returned-read/call normalization/lowering mechanics, so adding a family cannot silently omit a stage.

### READ-019 — Public matcher structs expose invalid intermediate states

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/matcher/mod.rs:19-46`, `glass-lint-core/src/api/rule/matcher/call.rs:8-17`, `glass-lint-core/src/api/rule/matcher/member.rs:11-20`, `glass-lint-core/src/api/rule/matcher/derived.rs:5-154`, `glass-lint-core/src/api/rule/matcher/flow.rs:328-336`

Provider callers can mutate raw `String`, `Vec`, provenance, index, and optional lifecycle fields after using the builders, bypassing the invariants that the validated rule boundary claims to own; `MemberCallMatcher` even exposes public fields and duplicate accessors. Keep matcher storage private and use validated semantic types such as symbol/member paths, argument indices, and non-empty alternatives; if serde needs raw shapes, separate wire declarations from validated rule types.

### READ-020 — Matcher construction errors lose domain structure

- **Severity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/api/rule/module.rs:7-49`, `glass-lint-core/src/api/rule/matcher/mod.rs:158-288`, `glass-lint-core/src/api/rule/matcher/flow.rs:353-405`, `glass-lint-core/src/api/rule/validation.rs:16-145`, `glass-lint-core/src/api/rule/error.rs:5-27`

Package constructors, matcher validation, object-flow builders, and limit validation return `String`, while `RuleBuildError::InvalidMatcher` merely wraps that text. Use typed `MatcherBuildError`/`ModuleSpecifierError` values with a field path and error kind, then convert them at the rule boundary; callers and tests can match stable semantics instead of parsing messages.

### READ-021 — Evidence normalization clones string keys into several parallel maps

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/evidence.rs:17-86`

Each `(MatchKind, String)` key is cloned into total-count, related-evidence, occurrence, and grouped maps, and lookups/sort keys allocate more copies. Use one owned `EvidenceKey` and one accumulator containing count, bounded occurrences, truncation, and related evidence; move each symbol into that map once and sort by borrowed keys.

### READ-022 — Finding assembly repeatedly clones and rescans evidence

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/findings.rs:34-91`, `glass-lint-core/src/lint/linter.rs:343-379`, `glass-lint-core/src/project/report.rs:96-102`

Finding construction clones all ranges, rescans and clones evidence for every surviving range, then project enrichment rescans all capabilities by rule ID for every finding and finally takes/rebuilds `EvidenceList` to deduplicate related evidence. Assemble findings within each capability/evidence group while references and range indices are available, attach related evidence in the same pass, and allocate only the final report DTOs.

### READ-023 — Source-position and pretty-report paths rescan long lines

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/diagnostic.rs:215-283`, `glass-lint-core/src/report.rs:303-379`

`SourceLineIndex::position` counts characters from the start of a line for every endpoint, which can become evidence-count times line length for minified input; pretty rendering separately finds each line with `split('\n').nth(...)` and allocates a `String` for every character cell. Add per-line UTF-8 column checkpoints or a sorted batch range conversion, reuse the line index in rendering, and represent ordinary cells as `char`/borrowed slices so only tabs and controls allocate.

### READ-024 — Rule selectors are stored raw and reparsed with divergent logic

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/lint/selection.rs:25-75`

`RuleSelector` claims to retain a parsed shape but stores only `String`: validation substitutes `placeholder`, while every match splits again and tests later segments with unordered `contains` calls (so selectors with multiple middle wildcards can accept out-of-order segments). Parse anchored wildcard segments once into a validated representation and implement matching over that structure.

### READ-025 — Budget lifecycle is reimplemented in cross-flow code

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/budget.rs:28-81`, `glass-lint-core/src/analysis/flow/cross/mod.rs:52-113`

`CrossBudget` duplicates `Budget`'s checked increment, hard limit, used count, and sticky exhaustion, while `SourceBudget` encodes another inverted exhaustion protocol (`exhausted` starts true and `stabilized` clears it). Compose the shared `Budget` and introduce a clearly named fixed-point convergence result for rounds, leaving projection telemetry as a separate counter.

### READ-026 — Project model ownership is split between the analysis root and project module

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/mod.rs:43-111`, `glass-lint-core/src/analysis/project/mod.rs:1-12`, `glass-lint-core/src/analysis/project/exports.rs:1-295`, `glass-lint-core/src/analysis/project/graph.rs:1-204`, `glass-lint-core/src/analysis/project/projection.rs:1-159`

`ProjectSemanticModel`, its state fields, `ExportResolution`, and link-input assembly live in `analysis/mod.rs`, while nearly all behavior is distributed through `analysis/project/*`. Move the model and its private assembly types into an owning `analysis/project/model.rs` and re-export only the small external surface from `analysis`; the module tree would then express the same project boundary described by the architecture documents.

### READ-027 — Test fixtures are bypassed and large production modules contain long inline suites

- **Severity:** Low
- **Category:** Testing
- **Location:** `glass-lint-core/tests/support/mod.rs:15-75`, `glass-lint-core/tests/compact_source.rs:21-56`, `glass-lint-core/tests/semantic_matching.rs:14-37`, `glass-lint-core/src/lint/linter.rs:488-981`, `glass-lint-core/src/project/tests.rs:1-631`

Integration tests have shared rule/environment/count helpers but several suites rebuild near-identical linters and assertions, while `linter.rs` devotes roughly half its 981 lines to an inline test module and project tests remain a large coordinator. Extend a small configurable fixture builder and move behavior suites into focused test modules, while retaining local helpers for genuinely distinct environments.

### READ-028 — Small duplicate helpers and stale prose add avoidable noise

- **Severity:** Low
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/aliases.rs:180-182`, `glass-lint-core/src/analysis/scope/model.rs:423-425`, `glass-lint-core/src/lint/findings.rs:113-116`, `glass-lint-core/src/analysis/matching/arguments.rs:1-20`

SWC span containment is implemented twice, `contains_range` only forwards to `SourceRange::contains` for tests, and the constrained-matching module repeats the same explanatory paragraph verbatim. Keep one syntax span helper, call the owning range method directly, and remove the duplicated prose.

## Systemic Themes

- **Canonical data needs reference semantics.** Source text, fact payloads, local paths, argument records, and request identities frequently cross a boundary by cloning into another owned model. Typed handles, `Arc` at true sharing boundaries, and short-lived borrowed views would reduce both ambiguity and memory churn.
- **Fixed-point engines need collection-owned transfer operations.** Summary propagation, source refinement, cross-flow requirements, and work queues clone or rebuild collections because callers manipulate raw `Vec`, `BTreeMap`, or `IndexSet` storage. Domain collections should own delta extension, forking, deduplication, and queue lifecycle.
- **Validation should be a state transition.** Project input and matcher declarations remain publicly mutable after normalization, so downstream code defensively revalidates. Validated private types would make invariants visible in signatures and remove repeated work.
- **Parallel semantic paths are the largest duplication risk.** Predeclaration versus provenance traversal, indexed versus constrained matching, and facts versus effect/projector call models all require synchronized semantic edits. Consolidating each pair behind one owner is more valuable than local clone removal alone.

## Open Questions

No unresolved question needs to block implementation. The recommended decisions are:

- **Keep `SourceFile` as the public wire/input type and introduce a private shared `AdmittedSource`.** `SourceFile` is serialized, publicly constructed, and re-exported as part of the filesystem-free project contract, so exposing `Arc<str>` there would leak an internal concurrency/ownership choice without strengthening caller-visible invariants. Admission should consume `SourceFile` and convert its `String` into shared `SourceText` once; that may perform the one internal allocation/copy, after which all analysis, caching, line indexing, and worker APIs should reuse the same allocation. This also leaves room to change the internal sharing strategy without another public API break.

- **Preserve byte-for-byte source equality in the artifact-cache identity.** Strict identity and fail-closed behavior make a digest alone inappropriate, even if collision risk is small; the cache key should retain the shared source allocation and compare the actual bytes before declaring a hit. A cached digest and length may be added only as a fast rejection/indexing layer, with byte equality as the final check, while pointer-equal shared text can take the cheap path. Given the 64-entry bound, first eliminating text copies is more important than replacing the cache's simple lookup policy.

- **Make validated matcher storage private; do not preserve post-construction mutation.** The rule builder is the natural validation boundary, and current matcher types do not need public fields for serde, so downstream providers should construct through builders and read through narrow accessors/iterators. If a future configuration format requires mutable raw declarations, add separate wire DTOs and convert them into validated matcher types rather than weakening the compiled API. Breaking direct struct-literal callers is preferable to retaining two mutable/validated meanings in the same type, and repository guidance explicitly permits updating all callers in one clean migration.

- **Move projection-dependent status and operation counts into the returned classification result.** `ProjectSemanticModel` should remain the matcher-independent, immutable output of linking; selected matchers and their flow exhaustion are properties of a projection run, not facts that should be written back into that model. Return a `ProjectClassification` (or equivalent) containing module classifications, projection status, and counts, then have report assembly combine it with the linked model's status/counts. Remove `Cell`-backed projection telemetry and `merge_projection_outcome` once all callers consume the returned result, rather than keeping both observation paths.

- **Assume repeated request specifiers may resolve differently by authored span and preserve every request ID.** The public resolution key explicitly includes importer, request kind, and source range, so collapsing equal specifier strings would discard part of the contract and could hide conflicting caller answers. Index `(specifier, role)` to a deterministic small collection of `ModuleRequestId`s, store resolutions by qualified request ID, and compare all relevant candidates when deriving a specifier-level identity. Equal outcomes may converge to one identity; missing or conflicting outcomes must continue to fail closed rather than selecting the first request.

## Coverage

Reviewed all 121 Rust files under `glass-lint-core/src` (32,194 lines) and all 10 Rust files under `glass-lint-core/tests` (2,977 lines), including public API and validation, parsing/lowering, scope collection and queries, value resolution, fact construction, indexed and constrained matching, local and cross-module flow, project sessions/linking, diagnostics/reporting, and unit/integration test organization. The review was static and read-only; no Rust source, tests, configuration, dependencies, or existing documentation were modified.
