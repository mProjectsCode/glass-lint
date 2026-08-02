# Codebase Readability Audit

## Summary

This audit covers the workspace's 442 Rust source files (82,972 lines), with focused full-context review of semantic model types, phase transitions, collection owners, large coordination functions, and every `into_parts`, `from_parts`, `into_map`, and similar representation-conversion seam found in production code.

The codebase already uses semantic IDs and phase types extensively. The main remaining readability problem is that several abstractions stop at the storage boundary: owners expose maps, tuple slices, mutable vectors, or positional parts, and callers then implement the domain operation. That makes invariants, ordering, completeness, and phase transitions harder to locate. The recommendations below favor behavior on the owning type, opaque domain collections, and `From`/`TryFrom` for genuine one-to-one conversions. A public `into_parts` should be exceptional; multi-object transitions are usually clearer as named consuming operations on the owner.

No source changes were made. Findings are ordered approximately by impact within each area.

## Findings

### Project analysis and linking

#### [x] READ-001 — Project resolution dismantles its phase owners

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/tables.rs:13-58`; `glass-lint-core/src/project/session/mod.rs:406-468`; `glass-lint-core/src/project/session/artifacts.rs:24-88`; `glass-lint-core/src/analysis/project/model.rs:105-188`

The local-to-resolved project transition dismantles `SourceTable` and `ResolutionTable` with `into_map`, reaches into `AnalysisArtifacts`, reconstructs module and request identities in the session, and clones the source map to populate a five-map `ResolvedLinkInputData`. The phase types therefore name storage but do not own the correlated transition or its invariants.

**Recommendation:** Give `ResolvedLinkInputData` one named, consuming construction path from the typed source, resolution, artifact, and authored-request owners. Perform validation and module/request identity construction inside that transition. Remove the raw-map conversions and the source-map clone.

**Fix Applied:** `ResolvedLinkInputData::from_resolved(sources, artifacts, outcomes)` is now the single consuming construction path, holding the typed `SourceTable`, `ResolutionTable`, and `AnalysisArtifacts` owners; outcome validation and module/request identity construction moved inside it. `SourceTable::into_map` and `ResolutionTable::into_map` were deleted; `ResolvedProject` stores `sources: SourceTable` instead of a cloned map.

#### [x] READ-002 — Graph decomposition exposes and clones its representation

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/state.rs:9-53`; `glass-lint-core/src/analysis/project/linker/graph.rs:80-97`; `glass-lint-core/src/analysis/project/linker/scc.rs:9-103`; `glass-lint-core/src/analysis/project/linker/export.rs:19-45`

`ModuleGraph::forward` exposes the complete adjacency map, after which free functions rebuild traversal state, component membership, and the condensation graph. `SccPartition` then exposes `components` and `order`, and export linking clones both before consuming them.

**Recommendation:** Let `ModuleGraph` own SCC decomposition and neighbor traversal, and give opaque `SccPartition` an ordered-component iterator. Keep adjacency, membership, and topological-order storage private and remove the full-structure clones.

**Fix Applied:** `ModuleGraph` now owns SCC decomposition (`scc_partition`) and opaque neighbor traversal (`neighbors`); `linker/scc.rs` was deleted. `SccPartition` keeps `components`/`order` private and exposes an ordered-component iterator; export linking iterates the partition via `mem::take` with no full-structure clones.

#### [x] READ-003 — Module identity keys are split to fit nested storage

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/matching/identity_map.rs:1-69`; `glass-lint-core/src/analysis/matching/occurrence.rs:430-503`

`ModuleIdentityMap` stores module and export names as nested maps even though its public vocabulary is `ModuleExportKey`. Insertions split the key with `into_parts`; merges iterate both map levels and reconstruct keys, so storage layout drives the algorithm.

**Recommendation:** Store a flat map keyed by `ModuleExportKey`. Put authoritative-export merging and conflict handling on `ModuleIdentityMap`, eliminating key disassembly and reconstruction.

**Fix Applied:** `ModuleIdentityMap` now stores a flat `BTreeMap<ModuleExportKey, ExportResolution>`. `ModuleExportKey::into_parts` and `get_parts` were removed; merges (`merge_star_from`, `merge_missing_from`) operate on the flat map and stay on the owner.

#### [x] READ-004 — Plan requirements expose their capability sets

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/compiler/requirements.rs:14-140`; `glass-lint-core/src/api/compiler/physical.rs:419-500`

`PlanRequirements` exposes its nested sets and flow flags to physical-plan validation, which manually inserts identities, enables local/cross-call flow, and updates project requirements. Similar capability relationships are encoded in normalization methods, so legal transitions are not centralized even if independent recomputation is intentional.

**Recommendation:** Keep the collections private and provide semantic operations such as requiring an identity, local flow, cross-call flow, or project capability. Retain physical-plan recomputation as an independent validation oracle; share only invariant-preserving mutation vocabulary, not the normalization derivation algorithm.

**Fix Applied:** `PlanRequirements` collections and flow flags are private; it now exposes semantic mutations `require_identity`, `require_local_flow`, `require_cross_call_flow`, `require_local_static_values`, and `merge_from`. Normalization and the independent `executable_requirements` validation oracle both build through the shared mutation vocabulary; the derivation algorithms stay separate.

### Scope, facts, and values

#### [x] READ-005 — Scope graph implements mutation-index behavior

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/scope/mutation_index.rs:14-55`; `glass-lint-core/src/analysis/scope/graph.rs:158-223`; `glass-lint-core/src/analysis/scope/graph.rs:458-493`

`MutationIndex` exposes its property-assignment, rooted-mutation, and dynamic-eval maps to `ScopeGraph`. The graph performs nested insertion, clearing, sorting, grouping, and prior-eval lookup directly, so mutation-index behavior is implemented by the broader graph rather than the type that owns the state.

**Recommendation:** Move record, finalize, clear, grouped-query, and prior-eval operations onto `MutationIndex`. Keep `ScopeGraph` responsible for coordinating scope and binding resolution, and make the index's maps private.

**Fix Applied:** `MutationIndex` now owns record (`record_property_assignment`, `record_rooted_mutation`, `record_dynamic_evals`), `finalize`, grouped queries, and `has_prior_eval`; all four maps are private. `ScopeGraph` coordinates scope/binding resolution and delegates to the index.

#### [x] READ-006 — Origin snapshots escape as raw hash maps

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/facts/origin_map.rs:14-131`; `glass-lint-core/src/analysis/facts/control.rs:164-255`

`OriginMap::snapshot` returns a cloned raw `HashMap`, and control-flow merging compares that map with the live owner, collects removal keys, and mutates the owner one item at a time. Branch-intersection semantics and restoration are split between the collection and a free function.

**Recommendation:** Introduce an opaque `OriginSnapshot<V>` and owner operations for snapshot, restore, and retaining origins common to branches. This keeps equality/intersection rules and storage choices together and removes `OriginMap::from(raw_map)` as a phase seam.

**Fix Applied:** Added opaque `OriginSnapshot<V>`; `OriginMap` now owns `snapshot`, `restore_from`, and `retain_common` (branch intersection). Control-flow merging uses these owner operations, and the `From<HashMap>` phase seams and the free intersection function were removed.

#### [x] READ-007 — Static objects are raw name-value tuples

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/value.rs:40-56`; `glass-lint-core/src/analysis/model/value.rs:143-188`; `glass-lint-core/src/analysis/model/fact.rs:327-357`; `glass-lint-core/src/analysis/flow/matcher.rs:89-166`; `glass-lint-core/src/analysis/flow/summary/parameter.rs:65-102`; `glass-lint-core/src/analysis/resolution/constant.rs:20-53`

Static objects are represented as `Vec<(NameId, ValueId)>`, and `ArgumentView` exposes them as a tuple slice. Property lookup, path traversal, deterministic iteration, and conversion are consequently reimplemented in matchers, summary projection, and constant resolution.

**Recommendation:** Add an opaque `StaticObject` domain collection owned by the value model. Give it property lookup, path traversal, stable iteration, and conversion behavior; pass its view through `ArgumentView` instead of a raw tuple slice.

**Fix Applied:** Added opaque `StaticObject` in the value model owning the private entry list, with `get`, `contains_key`, `value_at_segment`, and stable `iter`. `ArgumentView` now carries `Option<&StaticObject>` instead of a raw tuple slice; matcher, summary projection, and constant resolution use the collection's behavior.

#### [x] READ-008 — Binding slots are anonymous three-element keys

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:113-137`; `glass-lint-core/src/analysis/model/value.rs:179-187`; `glass-lint-core/src/analysis/flow/projector/mod.rs:150-162`; `glass-lint-core/src/analysis/flow/projector/mod.rs:777-790`

A binding slot is propagated as `(FunctionId, BindingId, NamePath)`, including as a `BTreeMap` key. `BindingKey` constructs the tuple, `ValueTable` repeats the return type, and the projector depends on positional meaning.

**Recommendation:** Introduce a named `BindingSlot` key in the scope/value model and return it unchanged across layers. The newtype should expose only meaningful access needed by diagnostics or projection.

**Fix Applied:** Added named-field `BindingSlot { function, binding, path }` in the scope/value model with the same `Ord`/`Hash` trait set as the tuple. `BindingKey::binding_slot` and `ValueTable::binding_slot` return it, and the projector stores `BTreeMap<BindingSlot, ValueId>`. All positional uses were removed.

#### [x] READ-009 — Scope freezing is expressed through generic parts records

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/binding_index.rs:15-58`; `glass-lint-core/src/analysis/scope/mutation_index.rs:14-34`; `glass-lint-core/src/analysis/scope/graph.rs:60-100`; `glass-lint-core/src/analysis/scope/build/freeze.rs:20-120`

Scope freezing has useful semantic subaggregates, but each is still assembled through generic crate-visible `Parts` records and `from_parts`. This exposes layouts to the freezer and makes the transition read like mechanical field wiring rather than construction of a validated frozen graph.

**Recommendation:** Give the scope collector one consuming `freeze` transition that constructs `ScopeGraph` and its subindices through private owner APIs. Use `From` internally for each one-to-one subindex conversion. Remove the visible parts records and generic `from_parts` APIs.

**Fix Applied:** `ScopeCollector::freeze` remains the single consuming transition; it now builds subindices through internal `From` conversions and `ScopeGraph::from_collected`. The visible `*Parts` records and generic `from_parts` APIs were removed, and binding/mutation fields are private.

#### [ ] READ-010 — Fact lowering dismantles both adjacent phase owners

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:228-305`; `glass-lint-core/src/analysis/resolution/mod.rs:150-160`; `glass-lint-core/src/analysis/scope/build/program.rs:1-16`

Local lowering cracks `ScopedProgram` and later `Resolver` into positional parts to freeze facts. Tests repeat the same builder/resolver disassembly, making the coordinator know which pieces must move together.

**Recommendation:** Introduce a `ResolvedProgram` phase aggregate that retains the resolver, scope-collection issues, and built fact state, then give it one consuming `freeze` transition. Keep `NameTable` and `ValueTable` inside that transition and remove both positional `into_parts` APIs.

**Fix Applied:** Added `ResolvedProgram` phase aggregate retaining the resolver, scope-collection issues, and built fact state, with one consuming `freeze` transition. `NameTable`/`ValueTable` stay inside the transition; `ScopedProgram::into_parts` and `Resolver::into_parts` were removed and tests go through the aggregate.

### Flow analysis

#### [ ] READ-011 — RequirementSet is neither requirement-specific nor opaque

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:147-309`; `glass-lint-core/src/analysis/model/flow.rs:316-416`; `glass-lint-core/src/analysis/flow/projector/history.rs:14-31`; `glass-lint-core/src/analysis/flow/projector/history.rs:120-185`

`RequirementSet` is also used for sink evidence, so its name obscures the shared abstraction. Its public `RequirementValues<K>` alias exposes `SmallVec`, and history recording converts values to `BTreeSet` for removal deltas, then reconstructs the collection on restore.

**Recommendation:** Rename the shared concept around indexed evidence, make its value collection opaque, and let it own readiness and remove/restore transitions. History should store the owner's semantic delta type rather than choosing a second collection representation.

**Fix Applied:** None so far.

#### [ ] READ-012 — Pending flow states delegate finalization to their caller

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:194-219`; `glass-lint-core/src/analysis/flow/projector/mod.rs:677-710`

`PendingFlowStates::take` returns its raw `BTreeMap`. The caller then groups paths in a `BTreeSet`, evaluates active-path completeness, assigns certainty, and emits finalized states, leaving the wrapper responsible only for storage.

**Recommendation:** Give `PendingFlowStates` a draining finalization operation that accepts a typed active-path set and returns named finalized records. Keep path grouping, completeness, and certainty derivation inside the owner and remove the raw-map `take` API.

**Fix Applied:** None so far.

#### [ ] READ-013 — Loop fixed-point completion mixes seven responsibilities

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:417-563`

`finish_loop` combines frontier coalescing, snapshot deduplication, replay, budget charging, break/continue extraction, convergence, and exit deduplication in one long coordinator. The data structures are named, but the fixed-point state and its legal transitions have no cohesive owner.

**Recommendation:** Introduce a loop fixed-point/frontier owner with operations for replay admission, convergence, and exit collection. Leave the projector method as high-level orchestration and keep budget outcomes explicit in the domain result.

**Fix Applied:** None so far.

#### [ ] READ-014 — Sink callers infer outcomes from collection mechanics

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/summary/sink.rs:42-152`; `glass-lint-core/src/analysis/flow/summary/summaries.rs:103-199`

Sink collection callers compare `len` before and after to infer insertion count, charge the budget externally for each addition, and use index ranges plus `get().expect(...)` despite an available iteration concept. Storage operations therefore stand in for domain outcomes.

**Recommendation:** Return a named collection outcome including the number inserted, and provide semantic projection iteration. Keep deduplication and insertion accounting on `SinkSet`, and remove its index-based `len`/`get` protocol.

**Fix Applied:** None so far.

### Matching and evidence

#### [ ] READ-015 — EventIndexView permits meaningless field combinations

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/matching/mod.rs:24-44`; `glass-lint-core/src/analysis/matching/mod.rs:134-201`; `glass-lint-core/src/analysis/matching/query/mod.rs:162-225`; `glass-lint-core/src/analysis/matching/query/view.rs:23-231`

`LinkedOccurrenceView` exposes six raw map buckets, while `EventIndexView` is assembled as a large bag of optional occurrence indices plus raw overlay and mask maps. Call sites must know which combinations are meaningful for each event, and lookup/overlay policy is distributed across free helpers and struct literals.

**Recommendation:** Replace the optional-field bag with an enum whose variants represent the supported event views. Give `LinkedOccurrenceView` semantic lookup and merge operations, and keep occurrence buckets, masks, and overlays private.

**Fix Applied:** None so far.

#### [ ] READ-016 — Rule evidence exposes mutable vectors

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/classification.rs:72-111`; `glass-lint-core/src/analysis/matching/arguments/mod.rs:52-61`; `glass-lint-core/src/analysis/flow/projector/state.rs:497-519`

`RuleEvidenceTable` provides `record` and `replace`, but also returns `&mut Vec` through `for_rule_mut`. Callers use it to append grouped evidence and scan or mutate truncation flags by event, bypassing the owner's vocabulary and any future ordering or deduplication invariant.

**Recommendation:** Add focused `extend`, grouped-record, and `mark_event_truncated` operations, then remove the mutable vector accessor. Preserve read-only iteration only where consumers truly need it.

**Fix Applied:** None so far.

### Reports, project configuration, and harness

#### [ ] READ-017 — Report APIs expose positional parts and mutable storage

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/report/analysis_report.rs:35-133`; `glass-lint-core/src/project/types/report/file_report.rs:5-48`; `glass-lint-core/src/project/report/mod.rs:19-87`; `glass-lint-core/src/lint/report/diagnostics.rs:45-75`; `glass-lint-cli/src/output.rs:470-487`

`AnalysisReport::into_parts` exposes a six-element positional tuple, and `FileReport` exposes both `diagnostics_mut` and its own `into_parts`. Combination and tests deconstruct reports, alter raw vectors, and reconstruct them, tying report evolution to tuple position and mutable storage access.

**Recommendation:** Treat reports as read-only public output contracts. Remove `into_parts` and mutable-vector access, make report assembly and aggregation crate-owned, and keep only inspection and supported serialization public. Cross-crate output tests should obtain reports through public analysis entry points rather than requiring public construction seams.

**Fix Applied:** None so far.

#### [ ] READ-018 — Tsconfig transitions use positional parts and compressed names

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Naming
- **Location:** `glass-lint-project/src/tsconfig/selection.rs:24-55`; `glass-lint-project/src/tsconfig/selection.rs:130-218`; `glass-lint-project/src/tsconfig/selection.rs:233-264`

`ParentSelection::into_parts` and `MergedSelection::into_parts` expose two- and four-element tuples across merge and compile phases. `merge_selection` also uses abbreviated names such as `m` and `pdir`, making an already positional transformation harder to follow; tests rely on ignored tuple positions.

**Recommendation:** Give `ParentSelection` a consuming merge operation and `MergedSelection` a consuming `compile(directory)` operation. Use `From` for their one-to-one internal field conversions, remove both `into_parts` methods, prefer `parent`, `merged`, and `parent_directory` to compressed names, and test semantic accessors instead of tuple slots.

**Fix Applied:** None so far.

#### [ ] READ-019 — Tool expectations are dismantled to qualify paths

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Encapsulation
- **Location:** `glass-lint-harness/src/types/case.rs:190-279`; `glass-lint-harness/src/cases/project.rs:210-250`

`ToolExpectation::into_parts` returns a selector and two finding vectors. The project-case loader applies the same default-path transformation to required and forbidden findings, then reconstructs the expectation.

**Recommendation:** Add a consuming `qualify_for_file` operation to `ToolExpectation`, backed by one path-qualification method on `FindingExpectation`. Preserve the required/forbidden distinction inside the owner and remove `into_parts`.

**Fix Applied:** None so far.

#### [ ] READ-020 — Profile summary finalization duplicates accumulator layout

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation
- **Location:** `glass-lint-harness/src/profile/types.rs:458-488`; `glass-lint-harness/src/profile/runner/summary.rs:17-63`; `glass-lint-harness/src/profile/runner/projects.rs:17-61`

`ProfileSummaryAccumulator` owns recording logic but exposes every total and the workload-result vector. Two consumers manually assemble closely related `ProfileSummary` values by reading or moving those fields, so finalization and field mapping are duplicated.

**Recommendation:** Give the accumulator a consuming `finish(ProfileSummaryMetadata)` operation that constructs `ProfileSummary`, with workload-specific timings and identity carried by the named metadata value. Keep totals private and remove the duplicated field-by-field assembly.

**Fix Applied:** None so far.

#### [ ] READ-021 — ProfileLinter is a behavior-free transparent wrapper

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Newtype
- **Location:** `glass-lint-harness/src/profile/types.rs:458`; `glass-lint-harness/src/profile/runner/workers.rs:14-40`; `glass-lint-harness/src/profile/runner/projects.rs:68-86`; `glass-lint-harness/src/profile/runner/admitted.rs:35-110`

`ProfileLinter(pub Arc<Linter>)` adds no invariant or behavior, and every consumer immediately pattern-matches the tuple field. It creates a semantic-looking type without encapsulation and adds noise to worker loops.

**Recommendation:** Remove `ProfileLinter` and use `Arc<Linter>` directly throughout the profiling runner. Do not retain a wrapper that has no current invariant or behavior.

**Fix Applied:** None so far.

### Shared domain types

#### [ ] READ-022 — Canonical package identity is duplicated and discarded

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/types/package.rs:1-45`; `glass-lint-core/src/project/types/input.rs:52-99`; `glass-lint-core/src/api/rule/module.rs:7-69`

`PackageName` and `PackageSpecifier` both validate and store the same canonical package-root concept. `PackageSpecifier` parses through `PackageName` and immediately discards it with `into_inner`; module patterns repeat the conversion into `String`.

**Recommendation:** Make the existing public `PackageSpecifier` the sole canonical package-root value and remove `PackageName`. Store `PackageSpecifier` directly in the package form of `ModuleSpecifierPattern`, translating its construction error at the rule boundary; keep exact module patterns as strings because they accept a different grammar.

**Fix Applied:** None so far.

#### [ ] READ-023 — NameId exposes an unused numeric representation

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** API
- **Location:** `glass-lint-datastructures/src/name.rs:11-21`

`NameId` is documented and otherwise treated as opaque, but public `as_u32` exposes its backing representation and has no workspace caller. This expands the API without expressing a domain operation and invites persistence or ordering assumptions.

**Recommendation:** Remove `as_u32` from the public API. Put formatting and serialization at the owning boundary so the numeric representation remains an implementation detail.

**Fix Applied:** None so far.

## Systemic Themes

- **Semantic types are strongest at the leaves but often disappear during transitions.** IDs such as `NameId`, `ModuleId`, and rule-specific indices prevent accidental mixing, yet surrounding code frequently turns aggregates into maps, tuples, and vectors before doing meaningful work. Preserve the vocabulary through the whole phase transition.
- **A collection wrapper should own its algorithms, not only its allocation.** The recurring leaks are finalization, merging, intersection, certainty, ordering, insertion counts, and mutation history. Moving those operations onto the collection makes invariants discoverable and permits storage changes without touching callers.
- **Prefer conversions according to their semantics.** Use `From` for infallible one-to-one owned conversions, `TryFrom` for validated conversions, and a named consuming method for a transition coordinating multiple owners or runtime context. `into_parts` is appropriate only when decomposition itself is a stable domain operation; none of the reported uses meet that bar.
- **Consolidate only truly duplicated concepts.** The package-root types and the behavior-free `ProfileLinter` merit consolidation or removal. By contrast, checkpoint wrappers and separate requirement/sink indices encode non-interchangeability and should remain distinct even when their storage matches. `NamePath` and `SymbolPath` likewise describe different domains and should not be merged merely because both are path-shaped.
- **Broad mutable access is the most expensive convenience API.** Returning `&mut Vec`, raw maps, or public fields forces every caller to learn storage and invariant details. Focused owner methods generally reduce both calling code and the future compatibility surface.
- **Interior mutability is not a general readability problem here.** The reviewed `Arc`, `Cell`, and synchronization uses are concentrated in shared immutable configuration, budgets, caches, and profiling. No general-purpose `Rc<RefCell<_>>` ownership web was found.

## Decisions

- **Reports are read-only public outputs.** Report creation, mutation, combination, and phase conversion belong inside core/project code. Public consumers receive inspection methods and supported serialization, not constructors, mutable collections, builders, or positional decomposition. Cross-crate tests should exercise output with reports produced by public analysis entry points.
- **`PackageSpecifier` is the canonical package-root type.** Remove the crate-private `PackageName`; use `PackageSpecifier` in project inputs and in the package variant of `ModuleSpecifierPattern`. Preserve the distinct exact-pattern representation because exact authored specifiers have a broader grammar.
- **Requirements validation remains independently derived.** `executable_requirements` continues to recompute capabilities from the physical plan so it can detect normalization or lowering mistakes. `PlanRequirements` owns private, well-named mutation methods, but normalization and physical validation do not share the derivation algorithm.
- **`ProfileLinter` is removed.** Use `Arc<Linter>` directly. A type with no present invariant or behavior does not earn a semantic wrapper.
- **Positional decomposition is not part of the design vocabulary.** Replace the reported `into_parts` and generic `from_parts` seams with `From`, `TryFrom`, or a named consuming domain transition according to whether the conversion is infallible, validated, or coordinates multiple owners.
- **Distinct safety types stay distinct.** Requirement and sink indices, checkpoint wrappers, `NamePath`, and `SymbolPath` prevent meaningful category mistakes and are not consolidation candidates merely because their representations match.
- **Unused representation access is removed.** `NameId::as_u32` should not remain public without a current domain use; serialization or display must be implemented at the owning boundary.

## Open Questions

None. The architectural choices raised by this audit are resolved above.

## Coverage

- Read the repository guide, root architecture, every crate `ARCHITECTURE.md`, `TESTING.md`, and `CONTRIBUTING.md`.
- Inventoried all 442 Rust source paths across core, project, provider, CLI, harness, test-support, and datastructure crates (82,972 lines excluding `target`).
- Reviewed semantic newtypes, tuple structs, public and crate-visible fields, raw collection accessors, mutable collection access, phase/parts types, and conversion seams including `into_parts`, `from_parts`, `into_map`, and `into_inner`.
- Reviewed the largest production modules and functions for mixed abstraction levels, as well as matcher, flow, scope, project-linking, reporting, tsconfig, and profiling call chains in full context where findings were raised.
- Searched for duplicated parsing and validation models, TODO/FIXME markers, broad lint suppressions, `Rc`/`RefCell` ownership, panicking collection access, and abbreviated naming. Only evidence-backed, actionable items are listed above.
- Tests were not run because this audit changes documentation only. Final verification is limited to Markdown and diff integrity and confirming that only this report changed.
