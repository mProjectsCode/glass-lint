# Codebase Readability Audit

Audit date: 2026-08-03

## Summary

This audit replaces the previous historical checklist with 15 current, actionable findings: 3 High, 11 Medium, and 1 Low severity. The dominant issue is incomplete ownership: many semantic types name a domain concept but still expose their maps, vectors, tuples, or slices, leaving callers to perform the concept's validation, transformation, or lookup policy.

The highest-value sequence is to close the project phase boundary, unify provenance-alternative state, and give static-object data one explicit storage and conversion policy. The next tier consolidates qualified export identity, bound flow targets, scope phase storage, project timing/metrics state, and validated query collections. These changes should reduce code by deleting parallel carriers and repeated collection mechanics, not by compressing implementations or removing explanatory comments.

## Findings

### Encapsulation and Domain Ownership

#### [x] READ-001 — Analysis artifacts do not own the transition to a resolved project
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/session/artifacts.rs:43-90`, `glass-lint-core/src/project/session/mod.rs:276-349`, `glass-lint-core/src/project/session/mod.rs:405-420`, `glass-lint-core/src/analysis/project/model.rs:120-250`

`AnalysisArtifacts` exposes all three of its stores, so the session probes two maps to decide whether a source needs analysis and the project transition later destructures the stores itself. `ResolvedLinkInputData::from_resolved` followed by `ResolvedLinkInput::build` then returns a positional triple that `LocallyAnalyzedProject::resolve` immediately unpacks into the next phase, spreading one ownership transition across three types.

**Recommendation:** Make the artifact stores private and add domain operations such as `needs_analysis` and authored-outcome validation. Have `LocallyAnalyzedProject::resolve` perform one private consuming transition directly into `ResolvedProject`, and delete `ResolvedLinkInputData` plus the positional result from `ResolvedLinkInput::build`. Keep all intermediate transition state private.

**Fix Applied:** `AnalysisArtifacts` stores are now private; it owns the pending-source probe via `needs_analysis`, authored-outcome validation via `is_authored_request`, and one consuming `into_link_input` transition that validates outcomes, assigns module/request identities, and splits parse diagnostics. `LocallyAnalyzedProject::resolve` now calls that single transition directly into `ResolvedProject`, `ResolvedLinkInputData` was deleted, and `ResolvedLinkInput::build` consumes the validated pieces and returns only `ResolvedLinkInput` (no positional triple).

#### [ ] READ-002 — Provenance alternative state is duplicated and interpreted by callers
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/scope/build/history.rs:22-70`, `glass-lint-core/src/analysis/model/scope.rs:260-276`, `glass-lint-core/src/analysis/scope/build/assignments.rs:63-118`, `glass-lint-core/src/analysis/scope/query/bindings.rs:29-87`

`ProvenanceAlternatives` and `AliasAssignment` both store a provenance vector plus `unknown` and `joined` flags, while only the former retains `exhausted`. Construction copies individual fields between them, and multiple query sites reconstruct meanings such as preferred non-local witness, complete witnesses, known value, and ambiguous join from the exposed vector/flag combinations.

**Recommendation:** Retain one opaque alternative-set value inside `AliasAssignment` and let it own bounded insertion, join state, unknown/exhausted state, preferred-witness selection, and complete-witness iteration. Give `AliasAssignment` named constructors for ordinary writes and joined writes instead of accepting field assembly. If exhaustion is intentionally translated into another status, make that conversion explicit at the owner boundary rather than dropping it during a field copy.

**Fix Applied:** None so far.

#### [ ] READ-003 — Static-object semantics are split across incompatible raw collections
- **Severity:** High
- **Fix Complexity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/model/value.rs:40-90`, `glass-lint-core/src/analysis/model/scope.rs:229-233`, `glass-lint-core/src/analysis/scope/build/provenance.rs:116-153`, `glass-lint-core/src/analysis/scope/build/provenance.rs:288-306`, `glass-lint-core/src/analysis/scope/mod.rs:47-73`

Static objects appear as `Vec<(NameId, ValueId)>`, `Vec<NameId>`, `BTreeMap<NameId, NamePath>`, and text-keyed `ConstValue::Object` maps. Callers implement key projection and conversion between these forms; meanwhile `StaticObject::new` accepts arbitrary tuple storage and `get` silently makes insertion order the duplicate-key policy.

**Recommendation:** Introduce a crate-private opaque static-property collection that owns deterministic construction, bounds, lookup, key projection, and conversion. Use source-order last-write-wins for supported literal properties, spreads, and `Object.assign`, matching the existing constant evaluator; unsupported or unbounded shapes remain `Unknown`. Preserve readable phase-specific wrappers for text keys, interned values, and rooted values, and expose only the projections each phase actually consumes.

**Fix Applied:** None so far.

#### [ ] READ-004 — Callers still implement path algebra with escaped slices
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-datastructures/src/path/name_path.rs:15-117`, `glass-lint-core/src/analysis/value/identity.rs:14-85`, `glass-lint-core/src/analysis/scope/query/provenance/chain.rs:76-132`, `glass-lint-core/src/analysis/scope/query/provenance/chain.rs:218-272`, `glass-lint-core/src/analysis/scope/mutation_index.rs:86-101`

`Path` and `PathView` exist, but semantic code repeatedly drops to `segments()`, `get(1..)`, and indexed subslices to compare tails, walk prefixes, and query mutation indexes. `MutationIndex` reinforces the leak by accepting raw `&[NameId]` keys even though its domain is `NamePath`.

**Recommendation:** Extend the existing `PathView` with the missing prefix-at-length, suffix/tail comparison, and bounded-prefix iteration operations, then accept `PathView` at mutation-query boundaries. Change the crate-private `Environment` path-equivalence operation to accept `SymbolPath`; keep the interned `NamePath` plus `NameTable` adapter as the free coordinator because it spans independent owners. Keep `segments()` for genuinely generic iteration, but remove slice arithmetic from semantic call sites.

**Fix Applied:** None so far.

#### [ ] READ-005 — Linked occurrence overlays expose bucket representation to the remapping algorithm
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/matching/mod.rs:39-124`, `glass-lint-core/src/analysis/matching/mod.rs:210-278`, `glass-lint-core/src/analysis/matching/occurrence.rs:361-425`

`LinkedOccurrenceView` names the overlay concept, but its construction is a closure over raw `BTreeMap<ModuleExportKey, Vec<&[Occurrence]>>` aliases. The caller selects one of five maps, applies masking and global promotion, and pushes borrowed buckets; package lookup then accepts the same raw overlay map as an optional argument.

**Recommendation:** Make `LinkedOccurrenceView` own identity remapping, masking, global promotion, and operation counting through a crate-private construction method. Let `OccurrenceIndex` provide the minimal bucket lookup needed by that constructor rather than a generic `iter` used to reproduce its storage policy. Package candidate lookup should consume `LinkedOccurrenceView`, not an optional nested map.

**Fix Applied:** None so far.

#### [ ] READ-006 — Repetition aggregation exposes its vector and leaves reporting operations outside
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-harness/src/profile/types.rs:381-431`, `glass-lint-harness/src/profile/runner/projects.rs:25-50`, `glass-lint-harness/src/profile/runner/admitted.rs:57-86`, `glass-lint-harness/src/profile/runner/summary.rs:29-50`

`MeasuredRepetitionAccumulator` owns a public `repetitions` vector, but callers implement zip-merging, sums, median input selection, operation aggregation, and final extraction by reaching into it. The type consequently protects neither repetition-count alignment nor the set of derived measurements.

**Recommendation:** Keep the vector private and add `merge_project`, aggregate getters, `median_duration`, and consuming `into_repetitions` operations. Let each runner combine those domain results with its workload-specific metadata. Restrict raw iteration to unit tests.

**Fix Applied:** None so far.

#### [ ] READ-007 — Load progress and metrics duplicate counters and synchronize by field assignment
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-project/src/loader_phases.rs:96-146`, `glass-lint-project/src/loader_metrics.rs:158-186`, `glass-lint-project/src/loader.rs:350-366`, `glass-lint-project/src/loader.rs:468-475`

`LoadProgress` owns request, edge, and byte counters for budget checks, while `ProjectMetricsAccumulator` stores the same counters for reporting. `publish` copies the fields across, and the loader separately writes `metrics.files` and reaches into `metrics.timings`, so the metrics owner is mostly exposed storage.

**Recommendation:** Replace `LoadProgress` and `ProjectMetricsAccumulator` with one crate-private load-accounting owner for bounded counter updates and timing records, then derive the public immutable `ProjectLoadMetrics` snapshot from it. Expose only domain operations such as `admit_requests`, `record_edge`, `admit_source_bytes`, `record_files`, and phase timing; keep all counters private. Remove `publish` and all direct counter assignments.

**Fix Applied:** None so far.

### Duplicate Types and Phase State

#### [ ] READ-008 — Qualified export identity is repeated as a tuple and nested-map convention
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/project/state.rs:164-273`, `glass-lint-core/src/analysis/project/resolver.rs:98-141`, `glass-lint-core/src/analysis/project/linker/export.rs:299-314`

The export table, lookup cache, and recursion guard all mean “module plus export name,” but encode it through separate method parameters, nested maps, and `BTreeSet<(ModuleId, SmolStr)>`. The resolver repeatedly constructs and removes tuple keys while the cache separately tracks capacity and a manual entry count over the same identity.

**Recommendation:** Introduce a `QualifiedExportId` with `module` and `name`, and use it in resolver, cache, and recursion-guard APIs. Retain module-grouped storage privately inside `ExportTable` because identity projection consumes complete module export sets, while single-entry callers address entries through the qualified key. Do not reuse matching's `ModuleExportKey`, whose module component is an authored specifier rather than a linked `ModuleId`.

**Fix Applied:** None so far.

#### [ ] READ-009 — `LinkerOutcome` is a behavior-free copy of the final model's state
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/linker/mod.rs:26-60`, `glass-lint-core/src/analysis/project/linker/mod.rs:117-139`, `glass-lint-core/src/analysis/project/model.rs:278-369`

`ProjectLinker::finish` copies seven fields into `LinkerOutcome`, after which `ProjectSemanticModel::link_with_limits` immediately copies every field again and appends runtime limits and a trace arena. The intermediate type does not validate, transform, or expose behavior; it only creates a second field-by-field phase seam.

**Recommendation:** Change `ProjectLinker::finish` to consume the linker and construct `ProjectSemanticModel` directly, with the analysis limits supplied to it. Keep `ProjectLinker` as crate-private transient working state, and delete `LinkerOutcome` and its duplicate field wiring. Do not add a second public constructor for linker internals.

**Fix Applied:** None so far.

#### [ ] READ-010 — Mutable and frozen scope graphs duplicate storage and query logic
- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/graph.rs:26-85`, `glass-lint-core/src/analysis/scope/graph.rs:201-233`, `glass-lint-core/src/analysis/scope/graph.rs:265-454`, `glass-lint-core/src/analysis/scope/query/bindings.rs:21-48`

`ScopeGraph` and `FrozenScopeGraph` repeat the same four owner fields, and the frozen type adds a long facade of direct delegations. The strict `binding_at` selection algorithm is also duplicated between the collection-phase graph and the frozen query implementation, including direct interpretation of assignment alternatives.

**Recommendation:** Keep the two concrete `ScopeGraph` and `FrozenScopeGraph` types and move their common fields into one private `ScopeData` aggregate; a generic public phase parameter would add API without a current consumer. Keep only the collection-time queries required by `finish_collected_properties` on the mutable type, and keep the full semantic query surface on `FrozenScopeGraph`. Consolidating the provenance behavior from READ-002 should eliminate the duplicated `binding_at` branching.

**Fix Applied:** None so far.

#### [ ] READ-011 — Bound flow targets are represented as parallel maps in two indexes
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:24-181`, `glass-lint-core/src/analysis/flow/cross/sources.rs:134-165`, `glass-lint-core/src/analysis/flow/cross/graph.rs:73-74`, `glass-lint-core/src/analysis/flow/cross/propagation.rs:118-126`

`BoundFlowPlan` splits member and global sources and sinks into four maps with parallel insertion, normalization, and lookup paths. Cross-project source collection builds a second `SourceIndex` with the same member/global split, while `BoundFlowPaths` exposes `req_members` and is renamed by the behavior-free `FlowPathPlan` alias.

**Recommendation:** Bind `LifecycleCallTarget` once into a `BoundLifecycleCallTarget` enum containing either `NamePath` or a global name, then index sources and sinks by that type. Use one private normalized target-index owner for local planning and cross-flow source collection. Keep the descriptive `BoundFlowPaths` name, make its requirement paths opaque behind indexed iteration, and delete the `FlowPathPlan` alias.

**Fix Applied:** None so far.

#### [ ] READ-012 — Project timing snapshot and accumulator duplicate the same state
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-project/src/loader_metrics.rs:3-120`, `glass-lint-harness/src/profile/types.rs:215-325`

`ProjectPhaseTimings` and `ProjectPhaseTimingsAccumulator` contain the same seven durations inside `glass-lint-project`, and snapshotting copies every field. `ProfilePhaseTimings` repeats the shape across a crate boundary, but it also owns harness-only aggregation and is part of the harness's public summary API.

**Recommendation:** Delete `ProjectPhaseTimingsAccumulator` and let `ProjectPhaseTimings` use crate-private record and addition methods while retaining its existing narrow public getter API. Keep `ProfilePhaseTimings` as the harness-owned public type rather than widening the project crate's mutation API or introducing a cross-crate abstraction for one consumer. Convert project snapshots at the existing harness boundary.

**Fix Applied:** None so far.

#### [ ] READ-013 — `AnalysisWaveOutcome` is a one-field pass-through wrapper
- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Newtype
- **Location:** `glass-lint-project/src/loader.rs:218-221`, `glass-lint-project/src/loader.rs:350-365`, `glass-lint-project/src/loader.rs:424-432`

`AnalysisWaveOutcome` contains only `Vec<ResolutionRequest>`, and `analyze_wave` does nothing beyond wrapping `session.analyze_sources`; `process_wave` immediately unwraps the field. The name suggests a richer phase result but provides no invariant or behavior.

**Recommendation:** Delete `AnalysisWaveOutcome`, return `Vec<ResolutionRequest>` directly, and use the precise local name `requests`.

**Fix Applied:** None so far.

### Query Construction and Collection Invariants

#### [ ] READ-014 — Logical expression branches can bypass their validated constructors
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/rule/query/expression.rs:9-83`, `glass-lint-core/src/api/rule/query/expression.rs:202-249`, `glass-lint-core/src/api/rule/query/composition.rs:45-76`, `glass-lint-core/src/api/rule/query/composition.rs:85-164`

`AnyExpr` and `AllExpr` are semantic wrappers for non-empty, bounded, depth-checked branches, but their `pub(crate)` vectors allow direct struct construction. Three composition helpers use `AllExpr { branches }` rather than `AllExpr::new`, and one helper materializes `vars()` merely to perform a membership query already represented by `contains_var`.

**Recommendation:** Add one private `LogicalBranches` collection and route all construction through its validation. Keep the public `AnyExpr` and `AllExpr` names as thin semantic wrappers for the rule-authoring API, with no direct branch storage access. Use named expression queries such as `contains_var` instead of exporting a vector for caller-side set operations.

**Fix Applied:** None so far.

#### [ ] READ-015 — Lifecycle collections repeat non-empty, bounded, and canonicalization rules
- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:122-199`, `glass-lint-core/src/api/rule/query/lifecycle.rs:201-280`, `glass-lint-core/src/api/rule/query/lifecycle.rs:537-599`

Lifecycle conditions and completions store raw vectors inside enum variants and repeat collect, empty-check, capacity-check, sort, and dedup logic. `LifecycleQueryBuilder::build` then rechecks the empty and capacity invariants even though all current constructors already establish them; `pub(crate)` kind fields leave the type boundary weaker than its API promises.

**Recommendation:** Store opaque `LifecycleEvents` and `LifecycleSinks` collections that establish non-empty and bounded invariants once and apply an explicit canonicalization policy. Make `LifecycleCondition` and `LifecycleCompletion` fields private, leaving read-only semantic accessors for the compiler. Then let the builder validate only cross-stage requirements, such as configuration completion requiring a condition, rather than revalidating collection storage.

**Fix Applied:** None so far.

## Systemic Themes

- Several wrappers provide names but not ownership. A semantic type earns its place when it prevents invalid states or owns repeated domain operations; otherwise it should absorb the behavior or disappear.
- Phase transitions often copy fields through adjacent carrier structs. Prefer one consuming transition into the next valid state, with a named result only when multiple independent owners genuinely continue.
- Repeated `Vec` and `BTreeMap` mechanics cluster around real invariants: bounded alternatives, qualified identities, canonical event sets, target variants, and accumulated metrics. Those are the best candidates for domain collections.
- Phase distinctions remain valuable. Prefer concrete phase types sharing private storage over public generic state parameters, and do not make build-only mutation available after freezing or conflate authored identities with linked identities.
- The largest Rust files are not, by themselves, priority findings. Several are enlarged by cohesive unit tests or deliberately independent logical/physical reference evaluators; splitting them without a stronger ownership boundary would add navigation rather than readability.

## Decisions

- **Static-object duplicates:** Supported static object construction uses source-order last-write-wins. This matches JavaScript property overwrite behavior and the existing `BTreeMap::insert`/`extend` constant-evaluation path; unsupported, dynamic, or over-budget shapes remain `Unknown`.
- **Resolved project boundary:** `LocallyAnalyzedProject::resolve` transitions directly to `ResolvedProject`. `ResolvedLinkInputData` and positional resolved-parts output should be removed because there is no second consumer to justify them.
- **Scope phases:** Keep two concrete phase types and share one private `ScopeData`; do not introduce a public `ScopeGraph<State>` abstraction. The mutable graph keeps only the narrow queries needed to finish collection, while general semantic queries remain frozen-only.
- **Phase timings:** Consolidate the two project-crate timing types without expanding their public mutation API. Keep the harness timing type distinct because it owns harness-only accumulation and is already part of the harness's public report surface; do not add a shared crate or generic timing API.
- **API policy:** New collection and transition owners stay private or crate-private by default. Public methods are added only for an existing external caller and expose domain operations, not maps, vectors, iterators used for reconstruction, or speculative phase parts.

## Open Questions

None. The architectural choices raised by this audit are resolved in the decisions above and reflected in the affected recommendations.

## Coverage

- Reviewed all 441 Rust source files (83,561 lines by `wc -l`) across `glass-lint-datastructures`, `glass-lint-core`, `glass-lint-project`, provider crates, harness crates, output, and both CLIs.
- Read the root and crate architecture documents, `TESTING.md`, `CONTRIBUTING.md`, and the previous readability audit before evaluating current code.
- Inventoried tuple structs, semantic newtypes, collection aliases, public/internal collection fields, raw accessors, phase carriers, and direct field construction; then traced the highest-signal cases through their callers.
- Reviewed the largest production modules, error shortcuts, allow/expect usage, interior-mutability patterns, test organization, duplicate helpers, and stale-work markers. No `Rc<RefCell<_>>` pattern or actionable TODO/FIXME cluster was found.
- Excluded ordinary public DTOs, numeric ID newtypes, test-only inspection helpers, generic iterator exposure without domain policy, and intentionally independent logical/physical compiler reference evaluators.
- This audit changed only `CODEBASE_READABILITY_AUDIT.md`; no source, test, configuration, or other documentation changes were made.
