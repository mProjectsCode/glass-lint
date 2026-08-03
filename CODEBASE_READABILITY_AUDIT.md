# Codebase Readability Audit

## Summary

This audit replaces the previous report, which contained only checked-off historical findings. The current review focused on semantic newtypes, storage ownership, duplicated domain transformations, and opportunities to simplify APIs without making code terse. It covered the workspace Rust sources, the root and owning-crate architecture/testing guidance, and the existing audit history.

The most valuable remaining changes are to make resolution requests domain APIs instead of public records and give flow/evidence/parameter representations owners for their matching and rollback operations. The report contains 10 active findings:

- 2 high-priority encapsulation/API boundaries;
- 6 medium-priority ownership, duplication, or representation boundaries;
- 2 low-priority simplifications that become worthwhile after the larger migrations.

The audit itself was read-only. READ-001 was subsequently implemented as a
clean breaking migration and verified with `make ci`.

## Findings

### High priority

#### [x] READ-001 — `PathInterner` duplicated the `ParentPathStore` API

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Newtype, Duplication, Encapsulation
- **Location:** `glass-lint-datastructures/src/path_trie/store.rs`; `glass-lint-datastructures/src/path_trie/types.rs`

`PathInterner` owns exactly one `ParentPathStore` and forwards nearly every operation to it: append, parent/segment queries, edge lookup, concatenation, iteration, validity, and node counts. Both types are publicly re-exported, while the store also has linked-parent operations that the façade does not expose. Callers therefore choose between two names for closely related path storage and can accidentally select an API with different capabilities.

**Recommendation:** Consolidate on one domain type, preferably `ParentPathStore` renamed to a neutral `PathStore`, with explicit constructors for the default and bounded cases and methods for both local and linked appends. Preserve `PathId` ownership checks and typed link semantics during the migration. Read-only callers should borrow this store; do not add a separate frozen-store type or retain a second public forwarding façade.

**Fix Applied:** Replaced both public types with one `PathStore`, providing
default and bounded constructors plus local and typed linked-parent operations.
`PathId` no longer carries a linked bit or raw tagged-ID path, and all callers
now borrow the same store type for read-only access. `SummaryPathId` remains as
the intentional frozen/overlay distinction.

#### [x] READ-002 — Resolution requests expose their record storage to every phase

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** API, Encapsulation, Newtype
- **Location:** `glass-lint-core/src/project/types/input.rs:330-341`; `glass-lint-core/src/project/session/mod.rs:329-342`; `glass-lint-project/src/loader_phases.rs:41-48`; `glass-lint-project/src/resolver.rs:68-81`

`ResolutionRequestKey` and `ResolutionRequest` are public structs with public fields. Normalization mutates `key.importer` directly, session ordering reads four nested fields directly, and project resolution reconstructs importer/kind/specifier keys by reaching through the record. Tests and harness adapters also construct and inspect the literals, so the representation is now a cross-crate convention rather than an owned request abstraction.

**Recommendation:** Make the fields private and provide validated constructors plus semantic accessors such as `importer()`, `kind()`, `range()`, `specifier()`, and `key()`. Put request ordering and normalization on the owning types, or expose one named operation for each instead of requiring callers to sort and rewrite fields themselves. Migrate struct literals in tests and adapters in the same change so there is one construction path.

**Fix Applied:** Made request and request-key fields private, added validated
typed constructors and semantic accessors, and migrated core, project,
harness, and test callers away from struct literals and nested field reads.
Request ordering and resolution now consume the request abstraction directly.

#### [x] READ-003 — Resolution normalization is a free-function transformation over public internals

- **Severity:** High
- **Fix Complexity:** Medium
- **Category:** Encapsulation, API, Architecture
- **Location:** `glass-lint-core/src/project/input.rs:94-120`; `glass-lint-core/src/project/session/artifacts.rs:122-129`

`normalize_result` matches every `ResolverOutcome` variant and rewrites its nested values, while `normalize_resolution_key` mutates a request key in place. The consuming transition must remember to call both functions in the right order before inserting into `ResolutionTable`; the semantic types themselves do not enforce their normalized state.

**Recommendation:** Move normalization onto the owners, for example `ResolverOutcome::normalize` and `ResolutionRequestKey::normalize`, or make constructors produce normalized values and return the existing `ProjectInputError`. Keep `AnalysisArtifacts::into_link_input` as orchestration: validate authorship, invoke the domain operation, and insert the result. This also makes future variants less likely to bypass validation silently.

**Fix Applied:** Moved request-key and resolver-outcome normalization onto
their owning types as consuming operations, including validation of every
typed outcome variant. `AnalysisArtifacts::into_link_input` now orchestrates
those operations and authorship checking without mutating records through
free functions; the obsolete normalization helpers were removed.

### Medium priority

#### [ ] READ-004 — Flow planning has two owners for the same index and matching operations

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Duplication, API, Complexity
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:113-180`; `glass-lint-core/src/analysis/flow/planning.rs:182-293`

`BoundFlowPaths` provides static requirement enumeration and sink-index matching, while `BoundFlowPlan` provides near-identical methods for a `FlowId`. The plan builds and stores `req_members` from `BoundFlowPaths`, but the standalone type remains callable from cross-flow and projector code, so index reconstruction and argument interpretation have two entry points and two apparent owners.

**Recommendation:** Make one bound-plan type own requirement paths, typed requirement enumeration, and sink matching. Either embed `BoundFlowPaths` in the plan or move its useful operations onto a single plan/view type; remove the static helpers after migrating the cross-flow and projector callers. Keep `RequirementIndex` and `SinkIndex`, including the existing `Any` sink behavior, as the domain-level boundary.

**Fix Applied:** None so far.

#### [ ] READ-005 — Compiled flow fields leak matcher and index semantics across analysis modules

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation, API, Architecture
- **Location:** `glass-lint-core/src/api/compiler/object_flow.rs:30-38`; `glass-lint-core/src/api/compiler/object_flow.rs:104-108`; `glass-lint-core/src/api/compiler/object_flow.rs:155-199`

`CompiledObjectFlow`, its source/requirement/sink children, and sink arguments expose `pub(crate)` vectors and fields. Effect matching iterates sources and applies argument predicates itself, cross-flow indexing extracts source targets itself, summary collection iterates sinks and interprets `present_indices`, and projector code reads completion modes directly. This spreads the physical compiler representation across otherwise separate analysis owners.

**Recommendation:** Make the physical representation private and add only the small semantic operations required by current consumers: source matching/candidate iteration, sink argument membership, present-argument iteration, and completion predicates. Keep typed indexes at the flow boundary. Treat this as an internal evaluator contract, not a new public IR; do not add a general-purpose view layer or force the separate logical reference evaluator to share physical storage.

**Fix Applied:** None so far.

#### [ ] READ-006 — Generic evidence collections remain visible after lifecycle ownership was introduced

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Newtype, Encapsulation, API
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:141-164`; `glass-lint-core/src/analysis/model/flow.rs:234-347`; `glass-lint-core/src/analysis/model/flow.rs:385-443`; `glass-lint-core/src/analysis/model/flow.rs:520-564`

`LifecycleEvidence` now owns the requirement and sink stores, but its public-in-analysis surface still returns `EvidenceValues<E>` and `(index, values)` pairs. `IndexedEvidence` remains a public generic type with `usize` accepted through `EvidenceIndex`, and `FlowState` forwards the representation-shaped clear/restore/key methods. History consequently stores the generic collection rather than a named lifecycle rollback value.

**Recommendation:** Make `IndexedEvidence`, `EvidenceValues`, and the generic `EvidenceIndex` implementation details. Use an opaque lifecycle-owned rollback delta for history, and let lifecycle evidence own restore semantics; callers should not receive or construct the underlying value collection. Remove raw `usize` support and retain only `RequirementIndex`/`SinkIndex` at the flow boundary.

**Fix Applied:** None so far.

#### [ ] READ-007 — `ParameterBinding` fields and projection rules are repeated at call sites

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, Duplication, Complexity
- **Location:** `glass-lint-core/src/analysis/model/fact.rs:363-370`; `glass-lint-core/src/analysis/flow/summary/parameter.rs:9-61`; `glass-lint-core/src/analysis/flow/cross/worklist.rs:91-103`; `glass-lint-core/src/analysis/flow/summary/summaries.rs:313-327`

Although `ParameterBinding` is crate-internal, its fields are visible throughout analysis. Worklist seeding filters on `parameter_index` and empty `path`, summary projection repeats parameter/rest/path matching, and sink projection independently searches caller and target bindings. The type has a projection method, but the collection-level questions—root binding, compatible binding, and matching a sink path—remain duplicated outside it.

**Recommendation:** Hide the fields and add semantic methods on the binding and its owning summary/parameter collection, such as `root_for(argument_index)`, `matches_sink`, `is_invocation_compatible`, and `project_sink`. Centralize rest/default/path behavior there. Keep `ParameterBinding` distinct from effect `ParameterRef`: they encode different domains and should not be consolidated merely because both refer to parameters.

**Fix Applied:** None so far.

#### [ ] READ-008 — Artifact tables expose generic iteration for a transition they should own

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, API, Complexity
- **Location:** `glass-lint-core/src/project/session/artifacts.rs:23-44`; `glass-lint-core/src/project/session/artifacts.rs:136-154`; `glass-lint-core/src/project/tables.rs:12-34`

`AuthoredRequestTable` wraps a map but exposes `iter()`, and `SourceTable` does the same. `AnalysisArtifacts::into_link_input` then reconstructs module IDs with `sources.iter().enumerate()` and qualified request IDs by filtering authored records, matching importer paths, and constructing IDs at the call site. The wrappers preserve storage but do not own the domain transition that gives the entries meaning.

**Recommendation:** Put module-ID assignment and qualified-request-ID production on `AnalysisArtifacts`, `SourceTable`, or a named linker-input builder that owns both tables. Replace generic `iter()` with narrowly named operations only where a real domain query is required, such as stable source order or request IDs for a module. Keep report assembly separate if it needs a source-order view, but do not expose map iteration merely for convenience.

**Fix Applied:** None so far.

#### [x] READ-009 — `LoadAccounting` duplicates the complete storage of `ProjectLoadMetrics`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Newtype, Duplication, API
- **Location:** `glass-lint-project/src/loader_metrics.rs:87-141`

`ProjectLoadMetrics` and `LoadAccounting` each store the same timings, file count, request count, edge count, and byte count. `LoadAccounting::snapshot` manually copies every field into the other type, while all mutation methods are repeated on the mutable wrapper. This is a direct duplicate newtype rather than two representations with different invariants.

**Recommendation:** Use one private mutable metrics value and derive the immutable report view from it, or make `ProjectLoadMetrics` the owned state and return a clone at the reporting boundary. Preserve the current read-only getters and bounded admission methods, but eliminate the parallel field list and manual snapshot mapping. A small private mutation façade is enough if the public metrics type must remain immutable to callers.

**Fix Applied:** Removed `LoadAccounting` and made `ProjectLoadMetrics` the
single owned mutable metrics value. Crate-private recording and bounded
admission methods keep mutation inside the loader, while `snapshot` now
returns a clone of the same state for the public outcome.

#### [x] READ-010 — Generic `Path<S>` aliases leave semantic path transformations at call sites

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Newtype, Encapsulation, Naming
- **Location:** `glass-lint-datastructures/src/path/name_path.rs:8-84`; `glass-lint-datastructures/src/path/view.rs:12-45`; `glass-lint-core/src/analysis/scope/query/provenance/chain.rs:107-123`

`NamePath` and `SymbolPath` are aliases over a generic `Path<S>`, and both expose raw `segments()`. Scope provenance repeatedly turns borrowed views into paths, chains suffix segments manually, and iterates raw segments to implement semantic suffix and mutation operations. Some low-level segment access is appropriate for datastructure algorithms, but semantic callers have no named operation for “suffix after this prefix,” “append this chain,” or converting a view into the owning path.

**Recommendation:** Keep the existing shared path primitive and `PathView`; do not introduce separate `NamePath` and `SymbolPath` wrapper types speculatively. First add only the concrete operations already repeated by callers—such as `tail_after`, `suffix`, `append_chain`, and conversion from a view—and restrict new raw-segment use to datastructure algorithms. Revisit separate wrappers only if the two path domains later acquire different invariants.

**Fix Applied:** Added owned-path `from_view` and `suffix` operations to the
shared path primitive and replaced provenance's manual tail copying with those
operations. The generic path remains shared, while semantic callers now use a
named transformation owned by the path type; focused tests cover empty and
out-of-range suffixes.

### Low priority

#### [ ] READ-011 — Lowering and cache stage records expose phase storage instead of transitions

- **Severity:** Low
- **Fix Complexity:** Medium
- **Category:** Encapsulation, API, Architecture
- **Location:** `glass-lint-core/src/analysis/local.rs:86-107`; `glass-lint-core/src/analysis/local.rs:208-212`; `glass-lint-core/src/analysis/lowering/mod.rs:93-96`; `glass-lint-core/src/project/session/artifacts.rs:165-191`

`LocatedSourceContext`, `SharedSemanticArtifact`, and `LoweredSource` are stage records whose fields are read and reconstructed by session/cache code. The cache path reaches into `source_index`, `semantic`, `source.lines`, and `source_context().path`, so the cache and project phases know how the lowering result is physically split. These records are internal enough that the current design is not a public API break, but the field-oriented boundary makes future cache changes more expensive and obscures the intended transition between stages.

**Recommendation:** Add constructors and semantic accessors or consuming transitions such as `SharedSemanticArtifact::from_lowered`, `LoweredSource::with_source`, and `LocatedSourceContext::path`/`line_index`. Keep `LocalArtifact` as the owner of the paired source/semantic state and let cache insertion/reconstruction use named operations. Defer this cleanup until the higher-priority request and flow boundaries establish the preferred style for internal stage records.

**Fix Applied:** None so far.

## Systemic Themes

1. Several wrappers successfully hide their immediate storage but still expose `iter`, raw fields, or generic collection types. The next level of encapsulation is to own the transformation that callers repeatedly perform, not merely to add another forwarding method.
2. The code has both semantic newtypes and storage-oriented twins. The clearest consolidation candidates are `PathInterner`/`ParentPathStore` and `LoadAccounting`/`ProjectLoadMetrics`; `ParameterBinding` and `ParameterRef` should remain separate because their meanings differ.
3. Typed indexes are a good readability boundary, but they are undermined when neighboring APIs accept raw `usize` or reconstruct indices from vectors in multiple places. Keep index construction and argument interpretation with the owner of the indexed declaration.
4. Internal visibility still matters. `pub(crate)` fields can create the same coupling as public fields when multiple analysis subsystems manipulate the representation directly; privacy should follow the domain boundary, not only the crate boundary.
5. Simplification should target phase transitions and repeated semantic decisions. Removing comments, shortening expressions, or replacing explicit domain names with generic helpers would not address the problems identified here.

## Decisions

These decisions resolve the prior open questions and set the implementation boundary for follow-up changes:

1. **One path store, borrowed for read-only use.** Merge `PathInterner` and `ParentPathStore` into one storage owner with local and linked append operations. Do not create a frozen-store abstraction; `&PathStore` is the read-only API, and summary overlays may own another store when they need independent mutation.
2. **Compiled flow is an internal evaluator contract.** Keep its physical representation private and add only the narrow semantic methods required by current analysis callers. Do not expose a generalized flow view, promote the IR to a public API, or merge it with the separate logical reference representation.
3. **Rollback belongs to lifecycle evidence.** Replace `EvidenceValues` crossing the `FlowState`/history boundary with an opaque lifecycle-owned rollback delta. History may store and replay that delta, but it must not know the evidence collection's generic representation.
4. **Keep `PathView`; add no parallel path hierarchy yet.** `PathView` remains the existing borrowed utility. Add only proven path transformations to the current path API, and do not split `NamePath` and `SymbolPath` into new wrapper types until they have distinct invariants that justify the extra surface.
5. **Narrow API rule for all findings.** Prefer private fields and one named operation per repeated domain transformation. Do not add compatibility shims, generic iterator façades, or abstractions whose only evidence is that a future caller might need them. Each proposed method should replace an existing repeated call-site operation and receive focused tests with the owning type.

## Coverage

Reviewed the Rust workspace sources with emphasis on `glass-lint-core`, `glass-lint-datastructures`, and `glass-lint-project`, including their architecture documents, the root architecture/testing/contributing guidance, and the previous `CODEBASE_READABILITY_AUDIT.md`. The scan covered semantic models, path storage, project input and tables, flow planning/evidence, parameter projection, compiler flow records, lowering/cache transitions, and their production call sites.

The prior report's checked-off findings were treated as historical context, not as proof that all adjacent representation leaks were resolved. In particular, lifecycle consolidation, typed flow indexes, and earlier report-ordering fixes were resurfaced only where a remaining generic/raw boundary is still visible. Serialized protocol/report DTOs, provider manifests, and the intentionally separate logical/physical reference evaluator were excluded unless their surrounding internal representation was independently leaking into unrelated modules.

This is a read-only audit. No source or test files were modified, and no test command was run.
