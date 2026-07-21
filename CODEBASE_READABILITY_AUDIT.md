# Codebase Readability Audit

## Summary

The current review retains 23 actionable readability and maintainability issues across `glass-lint-core` and `glass-lint-project`: 8 high severity, 13 medium severity, and 2 low severity. Five items were removed because their stated fixes are complete: shared environment storage, synthetic call-argument constructors, shared project-relative paths, the CommonJS recorder consolidation, and moving owned analysis jobs into workers.

Several claimed fixes remain materially incomplete. The changes often remove one panic, clone, or auxiliary map while leaving the duplicated traversal or ownership model intact; the recommendations below therefore favor one authoritative representation, typed IDs, immutable shared policy, borrowed views, and collection-owned fixed-point operations over further local patches.

The larger architectural, high-severity, and public-API findings include implementation guardrails. Those guardrails identify incomplete fixes to avoid, invariants that must survive the migration, obsolete paths that must be deleted, and concrete completion checks for the implementing agent.

## Findings

### READ-002 — Scope predeclaration and collection still duplicate the same AST traversal

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/predeclare.rs:24-190`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:117-156`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:298-406`, `glass-lint-core/src/analysis/scope/collect/mod.rs:426-469`

The two visitors independently decode imports and reproduce the function, arrow, block, loop, switch, `with`, and catch scope skeleton. The new plan packages scope indexes, but it is still consumed by a cursor in traversal order; divergence merely switches the second pass to empty fallback scopes. Introduce one scope walker or a typed `ScopePlan` keyed by stable node/span identities, with shared import and binding helpers, so both phases consume the same declared scope structure rather than relying on identical visit order.

**Implementation guardrails:** Do not retain two `Visit` implementations that merely call newly shared helpers; the list and nesting order of scope-forming syntax must have one owner. Scope identities cannot rely on spans being unique, so the plan needs stable traversal/node identities plus validated parent/kind information. Migrate both passes together, delete positional synchronization, and add adversarial tests for nested equal-span/generated nodes, hoisting, catches, loops, `with`, and unsupported divergence while preserving fail-closed behavior.

**Status:** Partial. `ScopePlan` replaces the named positional counter and makes divergence fail closed, but it stores only `scope_index` and is still matched by cursor order. Predeclaration and collection remain separate `Visit` implementations that each own the scope-forming traversal, so shared helpers and fallback behavior do not establish one scope-structure authority or stable node identity.

### READ-003 — Declaration handling repeatedly evaluates overlapping provenance models

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/collect/visitor.rs:35-115`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:158-217`

One initializer may be analyzed for mutable-object shape, declared-function identity, rooted aliases, derived patterns, bound callables, module aliases, `require`, constants, and returned objects, with several helpers recursively revisiting the same expression before one precedence branch wins. Give the collector a borrowed, lazy `DeclarationAnalysis` that caches shared subresults and applies precedence once; reuse the same analysis for assignment provenance where the current helper chain is repeated.

**Status:** Partial. `classify_declaration` now short-circuits the priority chain and shares the rooted path between its last two checks, reducing work on early matches. It does not introduce the borrowed/lazy `DeclarationAnalysis` requested by the finding: the separate helpers still evaluate overlapping expression/provenance models, and assignment analysis still has its own helper chain. The issue was made conditional, not consolidated.

### READ-004 — Resolver interior mutability forces broad records and arena values to be cloned

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:99-133`, `glass-lint-core/src/analysis/resolution/expression.rs:24-74`, `glass-lint-core/src/analysis/resolution/expression.rs:114-172`, `glass-lint-core/src/analysis/resolution/call.rs:158-199`, `glass-lint-core/src/analysis/resolution/constant.rs:7-34`

Cached `Arc<ResolvedValue>` hits still deep-clone the inner record, insertion clones it again, and recursive `ValueTable` queries call `.cloned()` before chasing `Binding`/`Callable` or materializing constants, so large static arrays and objects can be copied just to inspect a variant. Separate interning from immutable inspection, return stable resolution handles or narrow projections, and put recursive target/constant traversal on a borrowed `ResolverState`/`ValueTable` so one borrow owns the whole chase.

**Implementation guardrails:** Do not solve this by placing every `Value` or every cache result in a new `Arc`; that adds allocation and atomic-reference overhead while leaving the broad APIs and repeated projections intact. Establish an explicit mutable interning phase and immutable inspection API, with handles/views for callers that need only an ID, provenance, or path. Completion requires removing deep clones from cache hits and recursive arena traversal, with cycle, reassignment, ambiguity, and exhaustion tests proving the narrower APIs still fail closed.

**Status:** Partial. Identity-only fact-building paths now use `resolve_ident_id`, `resolve_member_id`, and `resolve_expr_id`, so cache hits do not clone the broad `ResolvedValue` for those consumers. `call_provenance_for_value` now follows binding/callable links through borrowed arena entries and clones only strings needed in returned provenance. Rich resolver APIs still deep-clone cached records, and constant materialization still clones composite shapes; stable resolution handles or a full immutable query phase remain outstanding.

### READ-005 — The canonical value arena is discarded while facts retain duplicate projections

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/value/arena.rs:19-48`, `glass-lint-core/src/analysis/facts/model.rs:135-158`, `glass-lint-core/src/analysis/facts/model.rs:207-305`, `glass-lint-core/src/analysis/facts/stream.rs:23-38`, `glass-lint-core/src/analysis/lowering.rs:209-245`

`ValueTable` already owns static strings, arrays, objects, rooted identities, and callable structure, but it dies with `Resolver`; `CallArgInfo` and fact variants therefore copy strings, keys, property values, provenance, paths, and projections into the retained stream. Freeze the value arena into `SemanticArtifact`/`FactStream` and let facts keep `ValueId`/`PathId` plus only irreducible event data, allowing matchers and flow passes to borrow value shapes instead of rematerializing them.

**Implementation guardrails:** The retained table should be immutable and artifact-local; later phases must not regain an interning back door or create self-referential facts borrowing the table. Pass the stream and frozen values together to consumers, resolve IDs through narrow borrowed accessors, and migrate matching, effects, and project projection in the same change. Delete duplicated string/key/projection fields once all consumers move—keeping them as a cache or compatibility path would leave two semantic authorities that can drift.

**Status:** Partial. Lowering now moves the immutable `ValueTable` into `SemanticFacts` alongside the retained stream, so `ValueId` has an artifact-local shape authority after `Resolver` is dropped. Matching and flow consumers still use copied fact payloads and do not yet borrow composite shapes through that table; duplicate projection/string fields remain and the full migration is outstanding.

### READ-006 — Argument construction still evaluates one expression through several pipelines

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/build/arguments.rs:19-82`, `glass-lint-core/src/analysis/facts/build/arguments.rs:87-174`, `glass-lint-core/src/analysis/resolution/expression.rs:231-259`

`arg_info` resolves the root once but still separately walks projections, evaluates object keys, evaluates each property string, falls back to another constant evaluation for the root string, and resolves the rooted chain; descendants are resolved again during projection collection. Add one bounded resolver-owned `ArgumentAnalysis` or borrowed constant-shape view from which keys, property values, paths, strings, provenance, and projections are derived once under one budget outcome.

**Implementation guardrails:** A new facade that internally invokes all existing evaluators is not sufficient; one walk/result must own constant shape, descendant values, rooted identity, and exhaustion state. Charge depth, node, name, and value budgets once and propagate one typed fail-closed outcome to every derived view. Preserve spread, dynamic-key, alias, reassignment, and minified-shape behavior with parity tests before deleting the old independent helpers.

**Status:** Partial. `arg_info` now passes the already-resolved root into some projection paths, so the common root is not resolved a second time. It still invokes separate projection, descendant, key, property-string, static-string, and rooted-chain evaluators; descendants are recursively re-evaluated and there is no shared result or single budget outcome. This is a reduced duplicate root lookup, not the resolver-owned one-walk analysis described above.

### READ-007 — Local fact paths retain both textual and interned representations

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/model.rs:207-218`, `glass-lint-core/src/analysis/facts/model.rs:235-305`, `glass-lint-core/src/analysis/matching/build.rs:154-243`, `glass-lint-core/src/analysis/flow/effect.rs:523-535`

`CallUnwrap`, `MemberRead`, and `Call` keep `SymbolPath` beside `NamePath`, and downstream matching and flow code still falls back to converting between them. Finish the local-path migration by interning every supported fact path during lowering and reserving `SymbolPath` for catalog, cross-artifact, and display boundaries.

**Implementation guardrails:** Pick one artifact-local representation and make absence/exhaustion explicit; do not keep textual fallback fields “temporarily” after callers migrate. Conversion from rule/catalog paths should happen once at the artifact boundary, while cross-module/report conversion should happen only when text is actually required. Completion means local matching, effects, and summaries contain no `NamePath::from_symbol_path` or reverse conversion on their hot paths.

**Status:** Partial. `NamePath` fields were added and some effect paths consume them directly, but `Call`, `MemberRead`, and `CallUnwrap` still retain textual `SymbolPath` fields alongside them. Matching still converts catalog `SymbolPath` values at use sites, and `matching/build.rs` retains `NamePath::from_symbol_path` conversions. The artifact-local authority has not been selected and the duplicate representation has not been deleted.

### READ-009 — Effect and projector layers still duplicate canonical call payloads

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:37-123`, `glass-lint-core/src/analysis/flow/effect.rs:457-520`, `glass-lint-core/src/analysis/flow/projector/mod.rs:79-137`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:15-108`, `glass-lint-core/src/analysis/project/model.rs:340-353`

`EffectCall` copies event, chain, rootedness, target, result, provenance, and arguments from `FactPayload::Call`; `EffectUse` repeats chain/event data again, while the local projector creates another `SourceCall` and the project model scans every effect to recover a result already stored on the fact. Keep `FactStream` authoritative, store `FactId` plus genuinely derived parameter/value relations in effects, and make indexes such as result-to-call map to `FactId` so all consumers borrow the payload directly.

**Implementation guardrails:** Effects should own IDs and derived relations, not references into the stream, so artifacts remain movable and avoid self-referential lifetimes. Centralize effective-call selection—including `.call()`/`.apply()` unwrapping—in a borrowed fact view used by local flow, summaries, and cross-module flow. Migrate all consumers and remove payload-bearing effect accessors together; a new `SourceCall` cache containing the same chain/provenance data would only rename the duplication.

**Status:** Not fixed. The helper extraction centralizes effective-call selection, but `EffectCall` still owns chain, rootedness, result, provenance, and derived arguments copied from the call fact. `EffectUse` repeats chain/root data, the projector still builds a payload-bearing `SourceCall` cache, and `source_call_result` scans effects to recover a result already available on the fact. The remaining work is to retain `FactId` plus derived relations and borrow canonical payloads through one fact view.

### READ-010 — Function-summary fixed points clone caller and target state each round

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:101-121`, `glass-lint-core/src/analysis/flow/summary.rs:211-249`, `glass-lint-core/src/analysis/flow/summary.rs:252-309`

Summary construction copies effect parameters, direct-sink collection clones every call vector, and each propagation round clones caller calls/parameters plus every compatible target's parameters and complete sink set before mutating the caller. Use stable indexed tables with round snapshots or sink deltas, borrowing immutable call/parameter data and owning only newly discovered projections.

**Implementation guardrails:** Define round semantics explicitly: readers see the previous stable snapshot or a monotone delta, and writers append only deduplicated new sinks for the next state. Do not hide full-summary clones inside `Arc`, `Cow`, or helper-returned vectors; immutable call and parameter tables should remain borrowed throughout a round. Preserve deterministic ordering and the hard round bound, and test direct, recursive, and mutually recursive helpers for identical evidence and exhaustion behavior.

**Status:** Partial. The implementation no longer clones an entire `FunctionSummary`, but each round still clones every caller's call IDs and parameter bindings and clones each compatible target's parameters and complete sink set. `collect_direct_sinks` also materializes per-function call vectors. The fixed point still needs stable indexed tables or round-local sink deltas so immutable caller/target data remains borrowed.

### READ-011 — Cross-flow source refinement repeatedly clones, sorts, and deduplicates whole buckets

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:51-78`, `glass-lint-core/src/analysis/flow/cross/mod.rs:263-307`, `glass-lint-core/src/analysis/flow/cross/mod.rs:393-529`

`FlowSources::extend_from_key` clones an entire source bucket, `extend` sorts and deduplicates the whole destination on every edge, and up to 64 full-project rounds repeat the work under a bespoke inverted `SourceBudget` lifecycle. Use deterministic sets or round-local deltas and a shared fixed-point convergence result so only new candidates move between keys and exhaustion semantics are expressed once.

**Implementation guardrails:** Prefer a worklist/delta design in which insertion itself reports novelty; repeatedly sweeping every module and sorting whole buckets after each edge is the behavior being removed. The convergence abstraction must distinguish stabilized, operation-budget exhausted, and round-limit exhausted states without publishing partial evidence as complete. Add cyclic and high-fanout tests that assert deterministic order, bounded work, and the existing all-or-nothing fail-closed result.

**Status:** Partial and mischaracterized. `ContextWorklist` fixes the FIFO front-removal cost, but source refinement still runs up to 64 full project rounds. `FlowSources::extend_from_key` clones whole candidate buckets, and `extend` sorts/deduplicates the whole destination on every edge; `SourceBudget` still owns a bespoke round lifecycle. The worklist change did not implement the delta/set semantics required by this finding.

### READ-012 — Frozen path storage cannot represent summary paths without owned vectors

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/value/path.rs:52-110`, `glass-lint-core/src/analysis/value/path.rs:149-181`, `glass-lint-core/src/analysis/value/path.rs:212-255`, `glass-lint-core/src/analysis/flow/summary.rs:25-87`

`SummaryPath::join` and `without_first` allocate `Vec<PathSegment>`, while `PathInterner` finds both outgoing edges and a node's own segment by linearly scanning `by_parent` vectors; immutable concatenation is test-only and cannot extend the frozen table. Store canonical `(parent, segment)` entries in index-addressable storage and add a summary-local/composite path interner so fixed points carry compact handles and path inspection does not scan sibling edges.

**Implementation guardrails:** Keep the artifact's frozen path IDs immutable and introduce a separate bounded summary path domain rather than mutating canonical facts during projection. One storage structure should answer both ID-to-node and `(parent, segment)`-to-ID lookups; duplicating segment ownership across unrelated maps recreates synchronization risk. Preserve explicit path exhaustion and distinguish property names from numeric indices in joins, prefixes, and suffix removal tests.

**Status:** Partial. `PathInterner` now uses an addressable `(parent, segment)` edge map and stores each node's segment directly, removing sibling scans for lookup and ID-to-segment recovery. `SummaryPath::join`/`without_first` still materialize owned segment vectors, and there is no bounded summary-local/composite path domain yet.

### READ-013 — Occurrence queries allocate an intermediate vector for every clause

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:54-88`, `glass-lint-core/src/analysis/matching/query.rs:24-59`, `glass-lint-core/src/analysis/matching/query.rs:71-106`, `glass-lint-core/src/analysis/matching/query.rs:159-324`, `glass-lint-core/src/analysis/matching/arguments.rs:184-229`

Although exact occurrence buckets are borrowable slices and `Occurrence` is `Copy`, query helpers immediately call `to_vec`, package queries allocate base/overlay/merged vectors, and constrained evaluation consumes another candidate vector before producing final evidence. Return a borrowed/merged/filtering `CandidateOccurrences` iterator (with a typed scan fallback) and allocate only the final `ClassificationEvidenceOccurrence` collection.

**Implementation guardrails:** Use a small enum or concrete iterator composition for exact, overlay, merged, and filtered candidates so the optimization does not replace vector allocation with one heap-allocated boxed iterator per clause. Candidate selection and full clause evaluation must remain separate, with both constrained and unconstrained paths sharing the same semantic evaluator. Completion means exact indexed queries perform no intermediate allocation and package/overlay queries allocate only when the final evidence owner requires storage.

**Status:** Fixed. `CandidateOccurrences` provides indexed, merged, and scanned representations without a boxed iterator. Exact lookups borrow index slices directly, overlay lookups use the allocation-free merge iterator, and fallback scans remain explicit through `None`. Evidence collection allocates only at the final owned-evidence boundary. The focused matching tests and full workspace gate pass.

### READ-014 — Overlay lookup duplicates conversion logic and constructs owned lookup keys

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/arguments.rs:56-114`, `glass-lint-core/src/analysis/matching/mod.rs:243-304`

Result-value and module-export overlays repeat the same `LinkedModuleIdentity`-to-provenance conversion, while each argument/call lookup constructs a `ModuleExportKey` by cloning module and export strings. Put conversion on `LinkedModuleIdentity` and expose a borrowed tuple-key lookup or interned identity handle so predicates can query overlays without temporary owned keys.

**Status:** Fixed. `ModuleIdentityMap` now owns a two-level module/export table and exposes borrowed `get_parts` lookup. Argument overlay matching obtains borrowed provenance parts directly, while occurrence indexes retain their separate event-owned keys. The targeted overlay and constrained-matching tests pass.

### READ-015 — Evidence normalization rebuilds its grouped state and clones string keys per occurrence

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/evidence.rs:29-119`

After building one `BTreeMap<EvidenceKey, EvidenceAccum>`, normalization clones every string-bearing key into a flat vector, sorts/deduplicates it again, rebuilds a second grouped map, and looks back into the first map for counts and related evidence; the final sort clones symbols again. Normalize and bound each accumulator in place, retain borrowed/indexed keys for any global ordering, and track truncation per group plus overall group-limit truncation explicitly.

**Status:** Fixed. Normalization keeps one bounded accumulator map, transfers owned keys into the final DTOs without cloning them, sorts with borrowed symbol keys, and records per-group occurrence truncation separately from global group-limit truncation. Evidence normalization tests and the full workspace gate pass.

### READ-017 — Finding assembly rescans evidence and capabilities and clones report records

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/findings.rs:34-98`, `glass-lint-core/src/lint/linter.rs:314-380`

For each surviving range, `findings_for_capability` scans the entire range map and clones matching `Evidence`; project enrichment then scans all capabilities and their evidence again for every finding to rediscover related events by rule ID. Assemble located findings and related evidence in one capability-owned pass using range/group indices and stable `RuleIndex`, materializing only the final report DTOs.

**Implementation guardrails:** Keep semantic classification borrowed and matcher-independent until the final report boundary; do not introduce another intermediate owned finding model mirroring the public DTO. Associate related evidence by `RuleIndex` while processing its capability, and make range containment/grouping a single domain operation with deterministic ordering. Snapshot and adversarial nested-range tests should prove that the one-pass assembly preserves deduplication, counts, truncation, and related-event locations.

**Status:** Partial. The map removes the repeated capability scan and now accumulates all related evidence for a rule, preserving it for every finding of that rule instead of consuming it on the first finding. It remains an owned intermediate evidence model and clones report records before the final DTO boundary; a capability/range operation that owns only final report data is still outstanding.

### READ-018 — Pretty rendering rebuilds a full line layout and per-character strings for each finding

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/report.rs:316-417`

Every excerpt allocates `Vec<Cell>` for the complete source line, then `Cell::display` and `display_width` allocate `String`s for ordinary characters; repeated findings on one minified line therefore repeat work proportional to the full line length. Cache per-line display layout for a report or stream only the selected window, append ordinary chars directly, and measure a `char` without allocating a temporary string.

**Status:** Fixed. Direct writing and stack-allocated UTF-8 width measurement remove per-character temporary strings, and `PrettyFile` now caches each computed line layout for reuse by repeated evidence on the same line. Standalone `PrettyReport` rendering remains allocation-local by design; grouped report rendering uses the shared cache. Pretty-report tests and the full workspace gate pass.

### READ-019 — Matcher-family knowledge remains duplicated despite the family macro

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/matcher/mod.rs:17-118`, `glass-lint-core/src/api/rule/matcher/mod.rs:256-384`, `glass-lint-core/src/api/rule/normalization.rs:21-49`, `glass-lint-core/src/api/compiler/lowering.rs:79-300`

The macro generates only family views; the same twelve families still appear in `MatcherSet`, `Matcher`, `From` implementations, flattening, push, emptiness, normalization, and compiler lowering. Make one declaration generate storage, enum/conversions, visitation, and dispatch metadata, or replace the parallel representations with one typed matcher collection and family visitor.

**Implementation guardrails:** The chosen authority must make omission from validation, normalization, or lowering a compile-time error when a family is added. Do not add a new registry beside the existing fields/enums; generate or eliminate the old exhaustive lists and update every provider caller in the same breaking change. Add a contract test that enumerates every declared family through construction, validation, normalization, flattening, and compilation.

**Status:** Partial. The canonical family declaration now generates `Matcher`, family views, flattening, push dispatch, and emptiness checks. The family list is still repeated in conversions, normalization/validation, and compiler lowering, so adding a family can still compile while being omitted from one of those paths. Exhaustive validation, normalization, and lowering from one declaration remain outstanding.

### READ-020 — Public matcher declarations still expose invalid mutable states

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/matcher/mod.rs:17-44`, `glass-lint-core/src/api/rule/matcher/flow.rs:42-60`, `glass-lint-core/src/api/rule/matcher/flow.rs:137-149`, `glass-lint-core/src/api/rule/matcher/flow.rs:249-370`

Only selected call/member fields were privatized: `MatcherSet` still exposes raw family vectors and strings, while public flow/value enums permit empty alternatives, unnormalized member names, and arbitrary vectors that are validated only later. Keep validated matcher storage private and use non-empty/domain collections and semantic string/path types; introduce separate raw DTOs if mutable wire declarations are ever required.

**Implementation guardrails:** Separate untrusted construction data from validated runtime types instead of adding setters plus repeated downstream validation. Builders should either return validated values or retain a distinct builder type that cannot enter a compiled catalog; no public field or enum variant should bypass required invariants. Update all callers and delete compatibility constructors that recreate invalid intermediate states, as the repository explicitly permits a clean breaking migration.

**Status:** Partial. `StaticStringPredicate` data is now hidden behind a crate-private kind, but the finding covers the wider public matcher state. `MatcherSet` still exposes raw family vectors, and public flow/value variants still permit empty or unnormalized states that are checked later. No separate raw/validated construction boundary was introduced, so callers can still construct invalid runtime declarations through other public fields and variants.

### READ-022 — Validated project input converts maps to vectors and immediately rebuilds maps

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/input.rs:13-72`, `glass-lint-core/src/project/input.rs:75-132`, `glass-lint-core/src/project/session.rs:668-681`, `glass-lint-core/src/analysis/project/model.rs:81-160`

Admission builds canonical source/resolution maps, converts them back to vectors, duplicates module-ID assignment in two methods, and `ValidatedLinkInput::build` reconstructs module, authored-request, and resolution maps; session finishing also drains typed tables into those vectors first. Let `ValidatedProjectInput` own the canonical typed tables and stable module IDs directly, consume them into linking, and remove the test-only duplicate ID algorithm.

**Implementation guardrails:** `ValidatedProjectInput` must be a true state transition, not a wrapper around the same raw vectors; its fields should make duplicates, unknown importers, and unnormalized targets unrepresentable. Bulk `lint_project` and incremental `AnalysisSession` paths must converge on the same owned tables and consume them without revalidation or remapping. Preserve deterministic ID assignment and public DTO serialization, then remove map-to-vector adapters and duplicate module-ID helpers rather than retaining them for convenience.

**Status:** Fixed. `ValidatedProjectInput::from_maps` is the single internal transition that assigns deterministic module IDs. Bulk admission and incremental sessions both pass canonical `BTreeMap` tables into it; session table-draining and the duplicate test-only ID algorithm were removed. Public `ProjectInput::validate` still performs its intentional DTO conversion at the API boundary.

### READ-024 — Filesystem walking and boundary policy are implemented twice

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-project/src/corpus.rs:43-141`, `glass-lint-project/src/corpus.rs:144-223`, `glass-lint-project/src/discovery.rs:39-135`, `glass-lint-project/src/discovery.rs:251-322`

`SourceCorpus` and `ProjectDiscovery` separately configure `WalkDir`, apply exclusions/extensions/symlink policy, count visited entries/files, translate walk errors, canonicalize roots, and load source files, but only one path incorporates deadlines and selection membership. Extract one bounded walker and canonical source reader parameterized by inclusion/deadline policy, then build corpus and project selection on that authority.

**Implementation guardrails:** The shared engine must own traversal order, symlink handling, exclusion timing, visited/file budgets, canonicalization, and error conversion; leaving either public facade with a private fallback walker keeps policy drift possible. Model deadline and membership as injected policy/callbacks so the simpler corpus API can opt out without duplicating the loop. Add cross-facade contract tests over symlinks, excluded directories, unsupported extensions, boundary escapes, and exact budget edges before deleting both old walkers.

**Status:** Partial. `walk::collect_files` centralizes the `WalkDir` loop, entry filtering, traversal/file limits, optional deadline, and walk-error conversion. `SourceCorpus` now owns canonicalization, root membership, and bounded source decoding for both direct corpus loads and project discovery; global project-byte accounting and tsconfig membership still belong to the loader/discovery boundary. The duplicate walk and source-reader policy are reduced, but the broader boundary consolidation remains incomplete.

### READ-025 — Loader timings and progress counters have parallel representations

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-project/src/loader.rs:74-225`, `glass-lint-project/src/loader.rs:371-393`, `glass-lint-project/src/loader.rs:458-480`, `glass-lint-project/src/loader.rs:517-525`

The same eight duration fields exist in `ProjectPhases`, `ProjectLoadMetrics`, and `ProjectPhaseTimings` with manual conversions and addition, while `LoadCounters` duplicates request/edge/byte fields that are copied into metrics at several call sites. Embed one `ProjectPhaseTimings` value in metrics and make a single `LoadProgress` owner atomically enforce budgets and expose counters, eliminating field-by-field synchronization.

**Status:** Fixed. `ProjectLoadMetrics` embeds the canonical `ProjectPhaseTimings`, while one `LoadProgress` owner now performs source-byte and request budget checks, records edges, and publishes the public counter snapshot. The former `LoadCounters` owner and scattered counter updates were removed; loader tests and the full workspace gate pass.

### READ-026 — Resolution caching clones request identities, outcomes, and loader options

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:342-369`, `glass-lint-project/src/loader.rs:484-507`, `glass-lint-project/src/resolver.rs:16-70`

Each authored request clones importer/specifier into a second cache key, cache hits clone `ResolverOutcome`, misses clone it into the cache before moving another copy into core, and `ProjectResolver` owns a full cloned `ProjectLoadOptions` beside the loader's validated options. Reuse a canonical request identity or borrowed tuple key, share/intern resolver outcomes until the core session consumes them, and retain only shared policy or the small classification fields the resolver actually needs.

**Implementation guardrails:** Preserve the distinction between authored requests in core—even equal specifiers at different ranges remain separate contract records—while allowing the filesystem resolver cache to reuse an outcome for the same importer/kind/specifier lookup. Avoid changing `record_resolution` into an `Arc`-leaking public API; shared/cache-owned outcomes should be an internal loader concern and be materialized or consumed once at the core boundary. Resolver policy should be borrowed/shared from validated options, and tests must cover conflicting authored spans, internal/external classification, exclusions, aliases, and deterministic cache hits.

**Status:** Partial. Resolution caching now keys directly by the canonical `ResolutionRequestKey`, eliminating the duplicate importer/specifier key representation, and `ProjectResolver` borrows the loader's validated `ProjectLoadOptions` instead of cloning the policy. Cache misses still clone `ResolverOutcome` into the cache and cache hits clone it into the session's owned resolution table; shared outcome handles remain outstanding.

### READ-027 — Project tests duplicate fragile temporary-directory setup and cleanup

- **Severity:** Low
- **Category:** Testing
- **Location:** `glass-lint-project/src/tests.rs:19-44`, `glass-lint-project/src/tests.rs:61-134`, `glass-lint-project/src/tests.rs:137-259`

Nearly every test constructs a process-ID path under the global temp directory, manually removes it before and after the test, and repeats directory/file creation; cleanup is skipped on panic and the two source-budget tests duplicate the same fixture and first assertion. Add an RAII temp-project fixture with unique directories and small write/build helpers, and merge the narrower budget test into the complete partial-report contract.

**Status:** Fixed. `TempProject` now creates a unique process/atomic-sequence directory without deleting a pre-existing path, and its `Drop` implementation remains the sole cleanup path. The redundant aggregate-budget test was merged into the complete partial-report contract. The project-loader test suite passes.

### READ-028 — Core test helpers remain fragmented and the project coordinator is oversized

- **Severity:** Low
- **Category:** Testing
- **Location:** `glass-lint-core/tests/support/mod.rs:1-89`, `glass-lint-core/tests/compact_source.rs:21-56`, `glass-lint-core/tests/semantic_matching.rs:14-37`, `glass-lint-core/tests/scope_precision.rs:1-28`, `glass-lint-core/src/project/tests.rs:1-631`

Integration suites still redefine linter/environment/count helpers already present in `tests/support`, while `src/project/tests.rs` mixes cache/session tests and shared fixture construction despite having focused sibling test modules. Extend one configurable integration fixture and move project test factories plus cache cases into a `project/tests/support.rs` and focused modules, leaving local helpers only where semantics genuinely differ.

**Status:** Partial. Shared project factories, linter builders, resolution keys, and the fixture now live in `src/project/tests/support.rs`, reducing the coordinator substantially and leaving test-specific assertions in focused modules. Cache/session cases remain in the coordinator and some integration helpers are intentionally local, so the full test-organization migration is not yet complete.

## Systemic Themes

- **Canonical data is abandoned too early.** The value arena, project maps, fact call payloads, and summary paths could remain authoritative across later phases; discarding them creates strings, vectors, and semantic replicas that cannot be borrowed. Environment policy and report paths now have cheap shared storage and are no longer part of this finding set.
- **Interior mutability is compensating for phase design.** `RefCell` makes recursive resolver code easy to enter but prevents references from escaping, which broadens return types and pushes cloning into every caller. A mutable build phase followed by an immutable query phase would simplify both borrowing and APIs.
- **Fixed-point engines manipulate raw collections.** Function summaries and cross-flow sources clone whole vectors because their collections do not own snapshot/delta propagation. Domain collections should expose monotone delta transfer and a shared convergence outcome.
- **Partial fixes need completion checks.** A shared helper, an `Arc` around a broad record, or a new map can reduce one local cost while leaving the old semantic authority alive. The remaining findings require deleting the duplicate path or proving that the new representation owns the invariant.
- **Several “single authority” comments do not match the code.** Scope order, matcher families, fact/effect call data, filesystem traversal, and loader metrics each still have two or more synchronized representations; comments should follow consolidation, not stand in for it.

## Open Questions

- Should the retained semantic artifact freeze `ValueTable` directly, or should it freeze a narrower read-only value-shape table after resolver-only binding/cycle data is removed? Either choice should keep `ValueId` stable and eliminate copied value projections.
- Should `SourceCorpus` remain a public facade, or should it be removed/delegated behind the same canonical source reader used by `ProjectLoader`? The current shared walker does not settle that ownership boundary.

## Coverage

Reviewed all 122 Rust files under `glass-lint-core/src`, all 11 Rust files under `glass-lint-core/tests`, and all 8 Rust files under `glass-lint-project/src` (38,067 lines total). Coverage included public API/validation/compiler layers, parsing and lowering, scope collection and queries, name/value/path storage, resolution, fact and module-interface construction, occurrence and constrained matching, local and cross-module flow, project admission/session/cache/linking/reporting, filesystem discovery/loading/resolution/options/errors, and unit/integration test organization.

The review used repository architecture/testing/contribution guidance, a full file and hotspot inventory, targeted cross-reference and clone/allocation scans, and focused compilation/tests. The worktree already contained source and test changes under review; this pass modified only this Markdown report. `cargo check -p glass-lint-core` and all 11 `glass-lint-project` tests passed.
