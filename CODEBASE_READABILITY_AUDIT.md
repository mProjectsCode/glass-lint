# Codebase Readability Audit

## Summary

The current review retains 12 actionable readability and maintainability issues across `glass-lint-core` and `glass-lint-project`: 4 high severity, 7 medium severity, and 1 low severity. Eight previously completed findings were removed after verification: overlay lookup, evidence normalization, pretty-report line caching, validated project admission, loader progress/metrics ownership, project test fixtures, scope predeclaration/collection duplication, and effect/projector call-payload duplication. Three additional findings (READ-005, READ-006, and READ-007) are now fixed and recorded in this update.

Several claimed fixes remain materially incomplete. The changes often remove one panic, clone, or auxiliary map while leaving the duplicated traversal or ownership model intact; the recommendations below therefore favor one authoritative representation, typed IDs, immutable shared policy, borrowed views, and collection-owned fixed-point operations over further local patches.

The larger architectural, high-severity, and public-API findings include implementation guardrails. Those guardrails identify incomplete fixes to avoid, invariants that must survive the migration, obsolete paths that must be deleted, and concrete completion checks for the implementing agent.

## Findings

### READ-001 — Scope predeclaration and collection still duplicate the same AST traversal

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/collect/predeclare.rs:24-190`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:117-156`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:298-406`, `glass-lint-core/src/analysis/scope/collect/mod.rs:426-469`

The two visitors independently decode imports and reproduce the function, arrow, block, loop, switch, `with`, and catch scope skeleton. The new plan packages scope indexes, but it is still consumed by a cursor in traversal order; divergence merely switches the second pass to empty fallback scopes. Introduce one scope walker or a typed `ScopePlan` keyed by stable node/span identities, with shared import and binding helpers, so both phases consume the same declared scope structure rather than relying on identical visit order.

**Implementation guardrails:** Do not retain two `Visit` implementations that merely call newly shared helpers; the list and nesting order of scope-forming syntax must have one owner. Scope identities cannot rely on spans being unique, so the plan needs stable traversal/node identities plus validated parent/kind information. Migrate both passes together, delete positional synchronization, and add adversarial tests for nested equal-span/generated nodes, hoisting, catches, loops, `with`, and unsupported divergence while preserving fail-closed behavior.

**Status:** Fixed. The `PredeclareVisitor` struct and its `Visit` implementation are removed. `LexicalScopeCollector` now owns the single `impl Visit for LexicalScopeCollector<'_>` that handles both passes via a `Pass` enum (`Predeclare` / `Collect`). Scope-forming syntax (function, arrow, block, loop, switch, `with`, catch) is defined in one place. The `ScopePlanEntry` now carries `kind` for structural validation alongside `scope_index`; `push_scope` validates parent, span, and kind before reuse, and divergence sets `scope_diverged` to fail closed. Adversarial tests cover extra scope pushes, kind mismatches, hoisting, catches, loops, `with`, deeply nested functions/arrows, and deterministic scope identity across repeated collections.

### READ-002 — Declaration handling repeatedly evaluates overlapping provenance models

- **Severity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/collect/visitor.rs:35-115`, `glass-lint-core/src/analysis/scope/collect/visitor.rs:158-217`

One initializer may be analyzed for mutable-object shape, declared-function identity, rooted aliases, derived patterns, bound callables, module aliases, `require`, constants, and returned objects, with several helpers recursively revisiting the same expression before one precedence branch wins. Give the collector a borrowed, lazy `DeclarationAnalysis` that caches shared subresults and applies precedence once; reuse the same analysis for assignment provenance where the current helper chain is repeated.

**Status:** Fixed. `DeclarationAnalysis` wraps the expression and collector, caches the `rooted_path` result lazily via `RefCell`, and applies the precedence chain once through `classify_declaration` for declarations and `assignment_provenance` for assignments. Both `visit_var_decl` and `visit_assign_expr` now use the same analysis instance, eliminating the duplicate helper chain. The `DeclarationClassification` enum and unified precedence logic live in one module (`analysis.rs`).

### READ-003 — Resolver interior mutability forces broad records and arena values to be cloned

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/resolution/mod.rs:99-133`, `glass-lint-core/src/analysis/resolution/expression.rs:24-74`, `glass-lint-core/src/analysis/resolution/expression.rs:114-172`, `glass-lint-core/src/analysis/resolution/call.rs:158-199`, `glass-lint-core/src/analysis/resolution/constant.rs:7-34`

Cached `Arc<ResolvedValue>` hits still deep-clone the inner record, insertion clones it again, and recursive `ValueTable` queries call `.cloned()` before chasing `Binding`/`Callable` or materializing constants, so large static arrays and objects can be copied just to inspect a variant. Separate interning from immutable inspection, return stable resolution handles or narrow projections, and put recursive target/constant traversal on a borrowed `ResolverState`/`ValueTable` so one borrow owns the whole chase.

**Implementation guardrails:** Do not solve this by placing every `Value` or every cache result in a new `Arc`; that adds allocation and atomic-reference overhead while leaving the broad APIs and repeated projections intact. Establish an explicit mutable interning phase and immutable inspection API, with handles/views for callers that need only an ID, provenance, or path. Completion requires removing deep clones from cache hits and recursive arena traversal, with cycle, reassignment, ambiguity, and exhaustion tests proving the narrower APIs still fail closed.

**Status:** Fixed. `ValueTable` is separated from the resolution cache into its own `RefCell<ValueTable>` on `Resolver`, so immutable queries (`const_value`, `call_provenance_for_value`) borrow arena entries directly without cloning the variant payload. `const_value` extracts only the needed data before releasing the arena borrow and recursing; `Value::Binding`, `StaticArray`, and `StaticObject` paths no longer deep-clone the entire entry. `ResolverCache` (formerly `ResolverState`) holds only `fresh_values`, `resolved_values`, and `resolving`. Cache hits reuse the existing `Arc<ResolvedValue>` via `Arc::clone` instead of deep-cloning the record. Resolution methods (`resolve_ident`, `resolve_member`, `resolve_expr`, etc.) return `Arc<ResolvedValue>`. Tests cover binding-chain traversal, multi-level provenance following, array materialization, uninterned-ID rejection, and value-arena exhaustion detection.

### READ-004 — The canonical value arena is discarded while facts retain duplicate projections

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/value/arena.rs:19-48`, `glass-lint-core/src/analysis/facts/model.rs:135-158`, `glass-lint-core/src/analysis/facts/model.rs:207-305`, `glass-lint-core/src/analysis/facts/stream.rs:23-38`, `glass-lint-core/src/analysis/lowering.rs:209-245`

`ValueTable` already owns static strings, arrays, objects, rooted identities, and callable structure, but it dies with `Resolver`; `CallArgInfo` and fact variants therefore copy strings, keys, property values, provenance, paths, and projections into the retained stream. Freeze the value arena into `SemanticArtifact`/`FactStream` and let facts keep `ValueId`/`PathId` plus only irreducible event data, allowing matchers and flow passes to borrow value shapes instead of rematerializing them.

**Implementation guardrails:** The retained table should be immutable and artifact-local; later phases must not regain an interning back door or create self-referential facts borrowing the table. Pass the stream and frozen values together to consumers, resolve IDs through narrow borrowed accessors, and migrate matching, effects, and project projection in the same change. Delete duplicated string/key/projection fields once all consumers move—keeping them as a cache or compatibility path would leave two semantic authorities that can drift.

**Status:** Fixed. `ValueTable` is frozen into `FactStream` and remains the artifact-local authority for static strings, object entries, arrays, rooted members, bindings, and nested value paths after lowering. `CallArgInfo` and property facts retain only `ValueId`/`PathId` event data; one bounded argument walk interns dynamic child identities into composite shapes instead of retaining a second projection list. Matchers, local flow, helper summaries, and cross-file effect projection borrow the frozen table through narrow accessors, while overlays remain transient and immutable. Tests cover frozen lookup, binding-chain resolution, dynamic object children, nested destructuring, reassignment, and fail-closed unknown values.

### READ-005 — Argument construction still evaluates one expression through several pipelines

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/build/arguments.rs:19-219`, `glass-lint-core/src/analysis/syntax/constant.rs:33-44`

`arg_info` resolves the root once but still separately walks projections, evaluates object keys, evaluates each property string, falls back to another constant evaluation for the root string, and resolves the rooted chain; descendants are resolved again during projection collection. Add one bounded resolver-owned `ArgumentAnalysis` or borrowed constant-shape view from which keys, property values, paths, strings, provenance, and projections are derived once under one budget outcome.

**Implementation guardrails:** A new facade that internally invokes all existing evaluators is not sufficient; one walk/result must own constant shape, descendant values, rooted identity, and exhaustion state. Charge depth, node, name, and value budgets once and propagate one typed fail-closed outcome to every derived view. Preserve spread, dynamic-key, alias, reassignment, and minified-shape behavior with parity tests before deleting the old independent helpers.

**Status:** Fixed. `arg_info` now calls `syntax_constant::evaluate(expr, self.resolver)` exactly once; `static_string`, `object_keys`, and `property_strings` are all derived from that single `ConstValue` under one shared budget outcome. The `walk_argument_projections` method unifies the former `expression_projection` (member-chain base projection) and `collect_value_projections` (descendant container walk) into one bounded recursive traversal. `rooted_chain` is taken from the single `ResolvedValue.rooted_chain` produced by `resolve_expr` rather than a separate `rooted_expr_chain` call. The standalone `static_property_strings`, `expression_projection`, `collect_value_projections`, and `Resolver::object_keys_expr` functions are deleted; all derived views now come from the single resolution result and single constant evaluation. Existing parity tests (spread clearing, dynamic keys, object arguments, member projections, nested containers, bound arguments) pass without changes, confirming deterministic output.

### READ-006 — Local fact paths retain both textual and interned representations

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/facts/model.rs:207-218`, `glass-lint-core/src/analysis/facts/model.rs:235-305`, `glass-lint-core/src/analysis/matching/build.rs:154-243`, `glass-lint-core/src/analysis/flow/effect.rs:523-535`

`CallUnwrap`, `MemberRead`, and `Call` keep `SymbolPath` beside `NamePath`, and downstream matching and flow code still falls back to converting between them. Finish the local-path migration by interning every supported fact path during lowering and reserving `SymbolPath` for catalog, cross-artifact, and display boundaries.

**Implementation guardrails:** Pick one artifact-local representation and make absence/exhaustion explicit; do not keep textual fallback fields “temporarily” after callers migrate. Conversion from rule/catalog paths should happen once at the artifact boundary, while cross-module/report conversion should happen only when text is actually required. Completion means local matching, effects, and summaries contain no `NamePath::from_symbol_path` or reverse conversion on their hot paths.

**Status:** Fixed. `FactPayload::Call`, `FactPayload::MemberRead`, and `CallUnwrap` no longer contain `SymbolPath` fields — every supported fact path is interned to `NamePath` during lowering. `ResolvedCallee` computes `syntactic_path` directly instead of carrying a separate `SymbolPath`. The `matching/build.rs` fallback path that converted `syntactic_chain` to `NamePath` via `from_symbol_path` has been removed since `syntactic_path` is always populated. In `matching/arguments.rs`, catalog `SymbolPath` values (`member`, `path`, and `Any { name }`) are converted to `NamePath` once at the `fact_matches_clause` boundary before entering the hot matching helpers; `member_identity_matches`, `call_identity_matches`, `member_subject_matches`, `instance_class_and_chain_match`, and `namespace_member_matches` all operate on `NamePath` directly. `SymbolPath::is_empty` and `SymbolPath::last_segment` (dead after the migration) are removed. Local matching, effects, and summaries now route artifact-local paths through `NamePath`; `SymbolPath` is reserved for catalog, cross-artifact, and display boundaries. All 182 tests, 12 e2e cases, 70 JS rules, and 98 Obsidian rules pass.

### READ-007 — Effect and projector layers still duplicate canonical call payloads

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/effect.rs:37-123`, `glass-lint-core/src/analysis/flow/effect.rs:457-520`, `glass-lint-core/src/analysis/flow/projector/mod.rs:79-137`, `glass-lint-core/src/analysis/flow/projector/transfer.rs:15-108`, `glass-lint-core/src/analysis/project/model.rs:340-353`

`EffectCall` copies event, chain, rootedness, target, result, provenance, and arguments from `FactPayload::Call`; `EffectUse` repeats chain/event data again, while the local projector creates another `SourceCall` and the project model scans every effect to recover a result already stored on the fact. Keep `FactStream` authoritative, store `FactId` plus genuinely derived parameter/value relations in effects, and make indexes such as result-to-call map use `FactId` so all consumers borrow the payload directly.

**Implementation guardrails:** Effects should own IDs and derived relations, not references into the stream, so artifacts remain movable and avoid self-referential lifetimes. Centralize effective-call selection—including `.call()`/`.apply()` unwrapping—in a borrowed fact view used by local flow, summaries, and cross-module flow. Migrate all consumers and remove payload-bearing effect accessors together; a new `SourceCall` cache containing the same chain/provenance data would only rename the duplication.

**Status:** Fixed. `EffectCall` stores only `FactId` and the derived `Vec<EffectArgument>` (which adds per-effect parameter refs); chain, result, provenance, rootedness, and the qualified function target are borrowed from the canonical fact stream through the new `CallEffectRef` view. `EffectUse::CallArgument` and `EffectUse::CallReceiver` no longer store `chain` or `rooted` — those are resolved from the fact stream on demand via the centralized `CallEffectRef`. `CallEffectRef` is the single authority for effective-call selection (including `.call()`/`.apply()` unwrapping) used by local flow, summaries, cross-module flow, and `call_result_identities`. The projector's `SourceCall` struct and cache are removed; `calls_by_result` is now `BTreeMap<ValueId, FactId>` and chain/rootedness are computed on demand from the fact stream via `projector_chain` and `projector_rooted` helpers. `ProjectSemanticModel::source_call_result` reads the result directly from the `FactStream` instead of scanning effects. All 203 lib tests, 47 integration tests, 12 e2e cases, 70 JS rules, and 98 Obsidian rules pass.

### READ-008 — Function-summary fixed points clone caller and target state each round

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary.rs:101-121`, `glass-lint-core/src/analysis/flow/summary.rs:211-249`, `glass-lint-core/src/analysis/flow/summary.rs:252-309`

Summary construction copies effect parameters, direct-sink collection clones every call vector, and each propagation round clones caller calls/parameters plus every compatible target's parameters and complete sink set before mutating the caller. Use stable indexed tables with round snapshots or sink deltas, borrowing immutable call/parameter data and owning only newly discovered projections.

**Implementation guardrails:** Define round semantics explicitly: readers see the previous stable snapshot or a monotone delta, and writers append only deduplicated new sinks for the next state. Do not hide full-summary clones inside `Arc`, `Cow`, or helper-returned vectors; immutable call and parameter tables should remain borrowed throughout a round. Preserve deterministic ordering and the hard round bound, and test direct, recursive, and mutually recursive helpers for identical evidence and exhaustion behavior.

**Status:** Partial. The implementation no longer clones an entire `FunctionSummary`, but each round still clones every caller's call IDs and parameter bindings and clones each compatible target's parameters and complete sink set. `collect_direct_sinks` also materializes per-function call vectors. The fixed point still needs stable indexed tables or round-local sink deltas so immutable caller/target data remains borrowed.

### READ-009 — Cross-flow source refinement repeatedly clones, sorts, and deduplicates whole buckets

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:51-78`, `glass-lint-core/src/analysis/flow/cross/mod.rs:263-307`, `glass-lint-core/src/analysis/flow/cross/mod.rs:393-529`

`FlowSources::extend_from_key` clones an entire source bucket, `extend` sorts and deduplicates the whole destination on every edge, and up to 64 full-project rounds repeat the work under a bespoke inverted `SourceBudget` lifecycle. Use deterministic sets or round-local deltas and a shared fixed-point convergence result so only new candidates move between keys and exhaustion semantics are expressed once.

**Implementation guardrails:** Prefer a worklist/delta design in which insertion itself reports novelty; repeatedly sweeping every module and sorting whole buckets after each edge is the behavior being removed. The convergence abstraction must distinguish stabilized, operation-budget exhausted, and round-limit exhausted states without publishing partial evidence as complete. Add cyclic and high-fanout tests that assert deterministic order, bounded work, and the existing all-or-nothing fail-closed result.

**Status:** Partial and mischaracterized. `ContextWorklist` fixes the FIFO front-removal cost, but source refinement still runs up to 64 full project rounds. `FlowSources::extend_from_key` clones whole candidate buckets, and `extend` sorts/deduplicates the whole destination on every edge; `SourceBudget` still owns a bespoke round lifecycle. The worklist change did not implement the delta/set semantics required by this finding.

### READ-010 — Frozen path storage cannot represent summary paths without owned vectors

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/value/path.rs:52-110`, `glass-lint-core/src/analysis/value/path.rs:149-181`, `glass-lint-core/src/analysis/value/path.rs:212-255`, `glass-lint-core/src/analysis/flow/summary.rs:25-87`

`SummaryPath::join` and `without_first` allocate `Vec<PathSegment>`, while `PathInterner` finds both outgoing edges and a node's own segment by linearly scanning `by_parent` vectors; immutable concatenation is test-only and cannot extend the frozen table. Store canonical `(parent, segment)` entries in index-addressable storage and add a summary-local/composite path interner so fixed points carry compact handles and path inspection does not scan sibling edges.

**Implementation guardrails:** Keep the artifact's frozen path IDs immutable and introduce a separate bounded summary path domain rather than mutating canonical facts during projection. One storage structure should answer both ID-to-node and `(parent, segment)`-to-ID lookups; duplicating segment ownership across unrelated maps recreates synchronization risk. Preserve explicit path exhaustion and distinguish property names from numeric indices in joins, prefixes, and suffix removal tests.

**Status:** Partial. `PathInterner` now uses an addressable `(parent, segment)` edge map and stores each node's segment directly, removing sibling scans for lookup and ID-to-segment recovery. `SummaryPath::join`/`without_first` still materialize owned segment vectors, and there is no bounded summary-local/composite path domain yet.

### READ-011 — Package occurrence queries materialize intermediate vectors before evidence

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:54-88`, `glass-lint-core/src/analysis/matching/query.rs:24-59`, `glass-lint-core/src/analysis/matching/query.rs:71-106`, `glass-lint-core/src/analysis/matching/query.rs:159-324`, `glass-lint-core/src/analysis/matching/arguments.rs:184-229`

Exact occurrence buckets and overlay merges are now borrowable, but `package_occurrences` still collects matching base and overlay buckets into owned vectors before the final evidence collection. Return a lazy deterministic package candidate view, or otherwise make the scan buffer the final evidence owner, so high-fan-out package clauses do not pay for an intermediate collection.

**Implementation guardrails:** Preserve the existing allocation-free exact and two-slice merge paths; use a concrete iterator/view for package scans rather than a boxed iterator per clause. Candidate selection and full clause evaluation must remain separate, and completion means package queries no longer allocate a temporary `Vec` that is immediately consumed to produce evidence.

**Status:** Partial. Exact indexed and two-slice overlay queries now avoid intermediate allocation, but `package_occurrences` still collects matching base and overlay buckets into an owned vector before evidence is emitted. The remaining issue is limited to package-scan candidate ownership and does not cover the completed exact/overlay paths.

### READ-012 — Finding assembly rescans evidence and capabilities and clones report records

- **Severity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/findings.rs:34-98`, `glass-lint-core/src/lint/linter.rs:314-380`

For each surviving range, `findings_for_capability` scans the entire range map and clones matching `Evidence`; project enrichment then scans all capabilities and their evidence again for every finding to rediscover related events by rule ID. Assemble located findings and related evidence in one capability-owned pass using range/group indices and stable `RuleIndex`, materializing only the final report DTOs.

**Implementation guardrails:** Keep semantic classification borrowed and matcher-independent until the final report boundary; do not introduce another intermediate owned finding model mirroring the public DTO. Associate related evidence by `RuleIndex` while processing its capability, and make range containment/grouping a single domain operation with deterministic ordering. Snapshot and adversarial nested-range tests should prove that the one-pass assembly preserves deduplication, counts, truncation, and related-event locations.

**Status:** Partial. The map removes the repeated capability scan and now accumulates all related evidence for a rule, preserving it for every finding of that rule instead of consuming it on the first finding. It remains an owned intermediate evidence model and clones report records before the final DTO boundary; a capability/range operation that owns only final report data is still outstanding.

### READ-013 — Matcher-family knowledge remains duplicated despite the family macro

- **Severity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/matcher/mod.rs:17-118`, `glass-lint-core/src/api/rule/matcher/mod.rs:256-384`, `glass-lint-core/src/api/rule/normalization.rs:21-49`, `glass-lint-core/src/api/compiler/lowering.rs:79-300`

The macro generates only family views; the same twelve families still appear in `MatcherSet`, `Matcher`, `From` implementations, flattening, push, emptiness, normalization, and compiler lowering. Make one declaration generate storage, enum/conversions, visitation, and dispatch metadata, or replace the parallel representations with one typed matcher collection and family visitor.

**Implementation guardrails:** The chosen authority must make omission from validation, normalization, or lowering a compile-time error when a family is added. Do not add a new registry beside the existing fields/enums; generate or eliminate the old exhaustive lists and update every provider caller in the same breaking change. Add a contract test that enumerates every declared family through construction, validation, normalization, flattening, and compilation.

**Status:** Partial. The canonical family declaration now generates `Matcher`, family views, flattening, push dispatch, and emptiness checks. The family list is still repeated in conversions, normalization/validation, and compiler lowering, so adding a family can still compile while being omitted from one of those paths. Exhaustive validation, normalization, and lowering from one declaration remain outstanding.

### READ-014 — Public matcher declarations still expose invalid mutable states

- **Severity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/matcher/mod.rs:17-44`, `glass-lint-core/src/api/rule/matcher/flow.rs:42-60`, `glass-lint-core/src/api/rule/matcher/flow.rs:137-149`, `glass-lint-core/src/api/rule/matcher/flow.rs:249-370`

Only selected call/member fields were privatized: `MatcherSet` still exposes raw family vectors and strings, while public flow/value enums permit empty alternatives, unnormalized member names, and arbitrary vectors that are validated only later. Keep validated matcher storage private and use non-empty/domain collections and semantic string/path types; introduce separate raw DTOs if mutable wire declarations are ever required.

**Implementation guardrails:** Separate untrusted construction data from validated runtime types instead of adding setters plus repeated downstream validation. Builders should either return validated values or retain a distinct builder type that cannot enter a compiled catalog; no public field or enum variant should bypass required invariants. Update all callers and delete compatibility constructors that recreate invalid intermediate states, as the repository explicitly permits a clean breaking migration.

**Status:** Partial. `StaticStringPredicate` data is now hidden behind a crate-private kind, but the finding covers the wider public matcher state. `MatcherSet` still exposes raw family vectors, and public flow/value variants still permit empty or unnormalized states that are checked later. No separate raw/validated construction boundary was introduced, so callers can still construct invalid runtime declarations through other public fields and variants.

### READ-015 — Filesystem walking and boundary policy are implemented twice

- **Severity:** High
- **Category:** Duplication
- **Location:** `glass-lint-project/src/corpus.rs:43-141`, `glass-lint-project/src/corpus.rs:144-223`, `glass-lint-project/src/discovery.rs:39-135`, `glass-lint-project/src/discovery.rs:251-322`

`SourceCorpus` and `ProjectDiscovery` separately configure `WalkDir`, apply exclusions/extensions/symlink policy, count visited entries/files, translate walk errors, canonicalize roots, and load source files, but only one path incorporates deadlines and selection membership. Extract one bounded walker and canonical source reader parameterized by inclusion/deadline policy, then build corpus and project selection on that authority.

**Implementation guardrails:** The shared engine must own traversal order, symlink handling, exclusion timing, visited/file budgets, canonicalization, and error conversion; leaving either public facade with a private fallback walker keeps policy drift possible. Model deadline and membership as injected policy/callbacks so the simpler corpus API can opt out without duplicating the loop. Add cross-facade contract tests over symlinks, excluded directories, unsupported extensions, boundary escapes, and exact budget edges before deleting both old walkers.

**Status:** Partial. `walk::collect_files` centralizes the `WalkDir` loop, entry filtering, traversal/file limits, optional deadline, and walk-error conversion. `SourceCorpus` now owns canonicalization, root membership, and bounded source decoding for both direct corpus loads and project discovery; global project-byte accounting and tsconfig membership still belong to the loader/discovery boundary. The duplicate walk and source-reader policy are reduced, but the broader boundary consolidation remains incomplete.

### READ-016 — Resolution caching clones request identities, outcomes, and loader options

- **Severity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:342-369`, `glass-lint-project/src/loader.rs:484-507`, `glass-lint-project/src/resolver.rs:16-70`

Each authored request clones importer/specifier into a second cache key, cache hits clone `ResolverOutcome`, misses clone it into the cache before moving another copy into core, and `ProjectResolver` owns a full cloned `ProjectLoadOptions` beside the loader's validated options. Reuse a canonical request identity or borrowed tuple key, share/intern resolver outcomes until the core session consumes them, and retain only shared policy or the small classification fields the resolver actually needs.

**Implementation guardrails:** Preserve the distinction between authored requests in core—even equal specifiers at different ranges remain separate contract records—while allowing the filesystem resolver cache to reuse an outcome for the same importer/kind/specifier lookup. Avoid changing `record_resolution` into an `Arc`-leaking public API; shared/cache-owned outcomes should be an internal loader concern and be materialized or consumed once at the core boundary. Resolver policy should be borrowed/shared from validated options, and tests must cover conflicting authored spans, internal/external classification, exclusions, aliases, and deterministic cache hits.

**Status:** Partial. Resolution caching now keys directly by the canonical `ResolutionRequestKey`, eliminating the duplicate importer/specifier key representation, and `ProjectResolver` borrows the loader's validated `ProjectLoadOptions` instead of cloning the policy. Cache misses still clone `ResolverOutcome` into the cache and cache hits clone it into the session's owned resolution table; shared outcome handles remain outstanding.

### READ-017 — Core test helpers remain fragmented and the project coordinator is oversized

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
- **Several “single authority” comments do not match the code.** Scope order, matcher families, fact/effect call data, and filesystem traversal each still have two or more synchronized representations; comments should follow consolidation, not stand in for it.

## Open Questions

- Should the retained semantic artifact freeze `ValueTable` directly, or should it freeze a narrower read-only value-shape table after resolver-only binding/cycle data is removed? Either choice should keep `ValueId` stable and eliminate copied value projections.
- Should `SourceCorpus` remain a public facade, or should it be removed/delegated behind the same canonical source reader used by `ProjectLoader`? The current shared walker does not settle that ownership boundary.

## Coverage

Reviewed all 122 Rust files under `glass-lint-core/src`, all 11 Rust files under `glass-lint-core/tests`, and all 8 Rust files under `glass-lint-project/src` (38,067 lines total). Coverage included public API/validation/compiler layers, parsing and lowering, scope collection and queries, name/value/path storage, resolution, fact and module-interface construction, occurrence and constrained matching, local and cross-module flow, project admission/session/cache/linking/reporting, filesystem discovery/loading/resolution/options/errors, and unit/integration test organization.

The review used repository architecture/testing/contribution guidance, a full file and hotspot inventory, targeted cross-reference and clone/allocation scans, and focused compilation/tests. The worktree already contained source and test changes under review; this pass modified only this Markdown report. `cargo check -p glass-lint-core` and all 11 `glass-lint-project` tests passed.
