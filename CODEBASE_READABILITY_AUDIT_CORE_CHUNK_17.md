# Codebase Readability Audit

Chunk 17 — `glass-lint-core` project linking
(`analysis/project/{identities,linker,model,projection,resolver,state}`)

## Summary

Chunk 17 is the project-linking pass: it takes validated module artifacts and
resolution tables, builds an internal module graph, partitions it into SCCs,
resolves export identities to a fixed point, caches negative and positive
export lookups, lays qualified identity overlays over matcher facts, and
projects the linked model into matcher-query evidence while recording bounded
completion metrics as status diagnostics.

The module split is mostly sound. Two-phase ownership (transient
`ProjectLinker` -> immutable `ProjectSemanticModel`/`LinkedProjectState`),
SCC-DAG export resolution, and the shared `ExportResolver` lookup layer are
cohesive, and the state types hide their maps behind narrow domain operations
rather than leaking storage. `ExportLookupCache` is genuinely shared with
`flow::cross`, so it is not dead architecture.

The main readability costs are: the same "does this plan need flow" predicate
is computed two ways; the retained field set of the linker is re-listed into a
second struct (`LinkedProjectState`) with a field-by-field handoff; the
`ExportResolver::from_maps` four-argument construction is wired up a second
time in `ProjectSemanticModel`; the projection pipeline is declared `pub`
although nothing outside `glass-lint-core` consumes it and nothing can (the
enclosing `analysis` module is crate-private); and `ProjectionMetrics` carries
a write-only `operations` counter with duplicated `record_*` accumulation.
Findings are ordered so that the owner consolidation (READ-002) precedes the
wiring deduplication that depends on it (READ-003).

## Findings

### Projection orchestration (`analysis/project/projection.rs`)

#### [x] READ-001 — "Does this plan need flow" is computed twice from different sources

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:174,207-209,371,382`

`ProjectionPlan::needs_flow()` returns `!self.flow_matchers.is_empty()`
(projection.rs:207-209) and decides whether effect summaries are loaded per
module in `ProjectionSession::project_modules` (projection.rs:174), while
`project_with_arena` independently computes
`let has_flow = plan.requirements.flow().local() || plan.requirements.flow().cross_call()`
(projection.rs:371) to decide whether the cross-module pass runs
(projection.rs:382). Both sources live on the same `ProjectionPlan` built in
`from_selection` (projection.rs:211-237) from the same physical root set: a
`PhysicalRoot::Lifecycle` pushes into `flow_matchers` (projection.rs:223-224)
and sets both flow flags together in `merge_requirements_into`
(`api/compiler/physical.rs:163-166`), so the predicates are equivalent today
but are expressed through two different artifacts (`flow_matchers` vs
`PlanRequirements.flow`). The only production reader of
`requirements.flow().local()/cross_call()` is the duplicate line itself; the
flags otherwise feed `#[cfg(test)]` plan summary/explanation
(`api/compiler/physical.rs:370-379,401-406`) and the requirements-mismatch
tests. The two consumers (`project_modules` and `project_with_arena`) can
therefore drift apart without a single owner keeping them in lockstep.

**Recommendation:** Keep one predicate on `ProjectionPlan` (the owner of the
lowered roots): make `needs_flow()` the solely authoritative check and delete
the inline `requirements.flow()` computation in `project_with_arena`, so both
the effect loading and the cross pass test the same boolean.
Guardrail: keep the `PlanRequirements.flow` flags intact as part of the
compiled plan contract (plan summary/explanation and the requirements-mismatch
tests read them); the runtime gate must stay true whenever a
`PhysicalRoot::Lifecycle` was lowered for a selected matcher.

**Fix Applied:** Reused `ProjectionPlan::needs_flow()` for the cross-module
projection gate, making the plan-owned lifecycle-root predicate authoritative
for both local effect loading and cross-flow projection. Compiled requirement
flags remain available to test-only plan summaries and validation.

### Linker ownership boundary (`analysis/project/linker/mod.rs`, `analysis/project/model.rs`)

#### [ ] READ-002 — `LinkedProjectState` re-lists the linker's retained field set field by field

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/linker/mod.rs:33-45,142-159` and `glass-lint-core/src/analysis/project/model.rs:237-245`

`ProjectLinker` (linker/mod.rs:33-45) and the post-link `LinkedProjectState`
(model.rs:237-245) share six identical fields (`modules`, `resolutions`,
`exports`, `link_cycle_rounds`, `diagnostics`, `status`); `finish()` manually
re-lists all of them plus a newly computed `edge_count` into the second struct
(linker/mod.rs:142-159), and the `#[cfg(test)]` `single_with_limits` path
constructs the same struct by hand again (model.rs:269-281). The retained
project state is therefore owned by two structs that must be kept field for
field in sync, and every future field that survives linking must be added to
the linker, the state struct, and both construction literals.

**Recommendation:** Give the retained semantic state a single owner: have
`ProjectLinker` hold (or wrap) a `LinkedProjectState` for the retained fields
— including `link_cycle_rounds`, which is already mutated in place during
linking (`linker/export.rs:56`) — while keeping only the transient items
(`graph`, `scc_partition`, `lookup_session`, `link_budget`, `link_limit`) as
linker-scoped fields, so `finish()` moves the state out directly instead of
copying it into a parallel literal. Guardrails: the transient graph/SCC/cache/
budget state must not survive into the immutable model; `edge_count` (from
`NormalizedModuleGraph`) must still be captured at the handoff; the immutable,
`Send + Sync` post-link model contract stays intact; the test-only
`single_with_limits` path keeps constructing `LinkedProjectState` directly.

**Fix Applied:** None so far.

### Export lookup layer (`analysis/project/resolver.rs`, `linker/mod.rs`, `model.rs`)

#### [ ] READ-003 — `ExportResolver` wiring is duplicated across the two owners and `ProjectLookupView` is a single-consumer view

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/resolver.rs:20-51,80-91`, `glass-lint-core/src/analysis/project/linker/mod.rs:48-58`, `glass-lint-core/src/analysis/project/model.rs:321-335`

The four-argument `ExportResolver::from_maps(&modules, &resolutions,
&exports, cache)` construction is written twice, once inside
`ProjectLinker::with_export_resolver` (linker/mod.rs:48-58) and once in
`ProjectSemanticModel::resolve_imported_identity` (model.rs:321-335). The
three maps are pulled from the parallel `modules`/`resolutions`/`exports`
field sets on the two owners (which READ-002 currently forces into sync),
while the cache is the linker's own `lookup_session` field but a
caller-supplied parameter on the model. Beneath it, `ProjectLookupView`
(resolver.rs:20-51) wraps the first two maps only; its `module()`/
`request_target()` methods are called exclusively from within `ExportResolver`
(resolver.rs:103,155,199,220,241), so the view is a single-consumer
intermediate rather than a shared API.

**Recommendation:** Inline `ProjectLookupView` into `ExportResolver`
(deleting the view struct and its constructor), then give both owners one
shared call path into the lookup layer so the `from_maps(...)` wiring exists
once — e.g. a small `with_export_resolver(cache, operation)` helper on the
retained project state from READ-002 that both the linker and the model call
with their respective caches. Guardrails: the linker must keep resolving
through the same bounded `ExportLookupCache` it already owns, the helper must
accept the cache as a parameter (the two owners hold different caches), and
the two distinct fallback behaviors (`target_to_export_resolution` with the
absent-target/`is_internal_request` fallback vs
`linked_target_to_export_resolution` for a known target) must stay separate.

**Fix Applied:** None so far.

### Projection public surface (`analysis/project/projection.rs`, `analysis/project/model.rs`)

#### [ ] READ-004 — Projection pipeline is published broader than its actual boundary

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:13,44,58,278,305,411,421-464`, `glass-lint-core/src/analysis/project/projection/outcome.rs:11`

`project_for_classification` (projection.rs:44), `assemble_classification_results`
(projection.rs:58), `ProjectMatcherModel` (projection.rs:278), and
`ProjectModuleHandle` (projection.rs:305) with its `modules()` iterator are all
`pub`, as are `ProjectionOutcome` (projection.rs:13, outcome.rs:11) and the
`projection` module itself (project/mod.rs:17). The only production caller of
the pair of free functions is `ProjectSemanticModel::classify_with_evidence_limit`
(model.rs:454-477), which consumes the model entirely inside `glass-lint-core`.
The handle + owner-token + `EvidenceQueryError` machinery (projection.rs:284-301)
is likewise only exercised in-crate, primarily by the misuse test
`analysis/tests.rs:89-138`. None of this surface can be reached from outside
the crate at all: the enclosing `analysis` module is crate-private
(`lib.rs:15`), so these `pub` items are not re-exported anywhere reachable
externally. The `pub` markers therefore only inflate the in-crate surface and
misrepresent the boundary, exposing internal wavefront internals (atomic global
token, handle/owner identity, error variants, `ProjectionOutcome`) that no
external crate can legitimately use and that the public API never has to
stabilize.

**Recommendation:** Shrink the pipeline to `pub(crate)` (the free functions,
`ProjectMatcherModel`, `ProjectModuleHandle`, `ProjectionOutcome`, and their
methods), keeping the `Result`-returning `evidence_for` reachable for in-crate
tests, and revisit whether the global atomic `ProjectMatcherIdentity` +
`ForeignModel` check is needed once the type can no longer escape the crate.
Guardrails: preserve the misuse-detection contract asserted by
`analysis/tests.rs:89-138` (querying a model with a handle from another model
must keep failing closed), and keep the evidence-order/determinism contract of
`evidence_for_checked`.

**Fix Applied:** None so far.

### Projection outcome accounting (`analysis/project/projection/outcome.rs`, `projection.rs`)

#### [x] READ-005 — `ProjectionMetrics::operations` is write-only and `record_local`/`record_cross` duplicate the same accumulation

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/projection/outcome.rs:89-102,139-168`, `glass-lint-core/src/analysis/project/projection.rs:173`, `glass-lint-core/src/analysis/project/projection/tests.rs:15`

`ProjectionMetrics::operations` (outcome.rs:101) is incremented in three
places (projection.rs:173 for overlay work, outcome.rs:157 and 167 in
`record_local`/`record_cross`) and is never read by any production code;
`lint/report/summary.rs:20-27` reads only the five narrower metrics fields via
the `ProjectionMetrics` getters (outcome.rs:181-199), and the aggregate
`AnalysisOperationCounts` has no total-projection-operation counter. `record_local`
(outcome.rs:139-158) and `record_cross` (outcome.rs:160-168) also repeat the
same flow-aggregation block: mark flow incomplete when exhausted, add the
source's operation count to `flow_operations`, add the source's heads to
`trace_heads`, and add the same count to the write-only `operations`. This is
leftover accounting that adds per-module/per-project accumulation for a value
nothing consumes, forcing reviewers to distinguish it from the `flow_operations`/
`flow_observed` counters that genuinely feed budget diagnostics.

**Recommendation:** Delete the write-only `ProjectionMetrics::operations`
field and its three accounting sites (including the vestigial
`metrics.operations = 100` set in projection/tests.rs:15), unless profiling
intends to surface a total-projection-operation count, in which case add a
getter and wire it into report operations instead. Extract the shared flow
accumulation (incomplete marking + `flow_operations`/`trace_heads` deltas)
into one private helper taking the source's `(is_incomplete, operations,
trace_heads)` triple, used by both `record_local` and `record_cross`.
Guardrails: keep `flow_observed`/`effect_observed`/`evidence_error`
reporting semantics, the `effect_projections` assignment in `record_cross`,
and the `flow_operations`-excludes-overlay-ops distinction asserted in
projection/tests.rs:3-20.

**Fix Applied:** Removed the write-only projection `operations` metric and its
overlay accounting. Shared flow completion, operation, and trace-head
accumulation now lives in `ProjectionOutcome::record_flow`, used by both local
and cross recording; report-facing metric getters and flow observation remain
unchanged.

### Export identity conversions (`analysis/project/linker/export.rs`, `analysis/project/identities.rs`)

#### [x] READ-006 — Static-string-vs-qualified `ExportResolution` construction is repeated

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/linker/export.rs:240-248,262-268` and `glass-lint-core/src/analysis/project/identities.rs:85-92`

`resolve_local_export` (export.rs:240-248, over the `Option<String>` computed
at export.rs:226-230) and `resolve_value_export` (export.rs:262-268) each map
a module interface's `static_string(name)` to `ExportResolution::StaticString`,
falling back to `Qualified { module, export }` with the identical `map_or_else`
shape. A third variant (identities.rs:85-92) builds `StaticString` from a
returned value's static string (via `stream.values().static_string(...)`) with
an `Unknown` fallback. The same low-level "optional static string to
`ExportResolution`, with a caller-chosen fallback" conversion is therefore
spelled out three times.

**Recommendation:** Extract one private constructor on the narrowest shared
owner (e.g. `ExportResolution` in `analysis/project/model.rs` or a helper in
`resolver.rs`) that takes the optional static string plus a fallback
resolution, and call it from all three sites so the `map_or_else`/`map_or`
shape and `to_owned` handling exist once. Guardrails: keep the `Qualified`
fallback for the two export.rs paths and the `Unknown` fallback for the
effect-return path, and preserve the Local-provenance precedence (static
string wins over qualified) inside `resolve_local_export`.

**Fix Applied:** Added the target-owned
`ExportResolution::from_optional_static_string` constructor and used it for
the qualified/static-string export paths and the local returned-value path.
Fallback identity selection and local-provenance precedence remain unchanged;
no inverse conversion or compatibility wrapper was introduced.
## Systemic Themes

- **Two-phase lifecycle re-encoding.** The transient-to-immutable boundary
  (RESOLVED `ResolvedLinkInput` -> `ProjectLinker` -> `ProjectSemanticModel`)
  is sound, but the retained fields are re-listed into `LinkedProjectState`
  (READ-002), and the lookup wiring built over that field set is re-constructed
  at the other owner (READ-003). Each new surviving field currently has to
  appear in two structs and two construction literals.
- **Predicates recomputed at different layers.** The same boolean ("plan runs
  flow") is derived from two artifacts on the same `ProjectionPlan`
  (`flow_matchers` vs `PlanRequirements.flow`, READ-001), and the same
  optional-static-string-to-`ExportResolution` conversion is re-spelled at
  three call sites (READ-006).
- **Public surface exceeds its reachable boundary.** The entire projection
  query pipeline (model, handle, identity token, error enum, `ProjectionOutcome`)
  is marked `pub` while all programmatic consumers live inside `glass-lint-core`
  and the enclosing `analysis` module is crate-private, so the markers only
  inflate the in-crate surface (READ-004).
- **Write-only accounting.** `ProjectionMetrics::operations` is accumulated
  per module and per project and never read; flanking `record_*` methods
  duplicate the same aggregation shape (READ-005).

## Open Questions — Resolved

1. **No cross-crate consumer exists for `ProjectMatcherModel`/`ProjectModuleHandle`,
   and none can.** The `analysis` module is crate-private (`lib.rs:15`,
   `mod analysis;`), so the `pub` projection types are never re-exported
   through a reachable path; a workspace-wide `rg` for `ProjectMatcherModel`,
   `ProjectModuleHandle`, `project_for_classification`, and
   `assemble_classification_results` matches only `glass-lint-core` files.
   Harness verification and profiling consume `AnalysisReport` and
   `AnalysisOperationCounts`, not projection types
   (`glass-lint-harness/src/profile/metrics.rs:10-25,36-38`), and the handle
   owner-identity contract is exercised solely by the in-crate misuse test
   (`analysis/tests.rs:89-138`). READ-004 therefore applies as written; there
   is no consumer to justify keeping the surface `pub`.
2. **Profiling does not surface a total projection-operation counter.**
   Profiling accumulates `AnalysisOperationCounts` from `report.operations()`
   (`glass-lint-harness/src/profile/metrics.rs:36-38`), and that aggregate
   type has no total-projection-operation field
   (`glass-lint-core/src/project/types/report/operations.rs:3-74`).
   `ProjectionMetrics` exposes no `operations` getter
   (`projection/outcome.rs:180-199`), and the CLI summary prints only the
   aggregate counts (`glass-lint-cli/src/output.rs:323-327`). Deleting
   `ProjectionMetrics::operations` per READ-005 is therefore safe; no getter
   or report wiring is owed.
3. **The `QualifiedRequestId` asymmetry is justified and should not be copied.**
   `QualifiedRequestId` is `pub` with private fields and a `pub(crate)`
   constructor (`model.rs:49-52,74-78`) because `project/session/artifacts.rs`
   — a `crate::project` module outside `analysis` — constructs it at
   `artifacts.rs:50,209`, where a `pub(in crate::analysis)` constructor would
   be unreachable. Its fields are never decomposed; every use passes the whole
   value as an opaque map key (`resolver.rs:49`, `model.rs:316-318`), so the
   doc note at `model.rs:46-48` is accurate. The sibling tokens
   `QualifiedFunctionId` (`model.rs:55-72`) and `QualifiedExportId`
   (`state.rs:217-237`) are `pub(in crate::analysis)` because they are never
   constructed outside `analysis`, and they decompose by design — their getters
   are read at `model.rs:341-343`, `identities.rs:80,86`, and
   `resolver.rs:194,199,216-217`. The no-decomposition note should not be
   extended to the siblings; the asymmetry is inherent, not a drift.
4. **`SccPartition`'s take/put-back dance is vestigial today.** `resolve_export_table`
   takes the partition (`export.rs:43`) and re-stores it (`export.rs:58`), but
   nothing reads it afterwards: `validate_imported_exports` (`mod.rs:124`) and
   `finish` (`mod.rs:142-159`) never touch it, and the only prior read is the
   `is_some()` gate in `build_graph_and_exports` (`mod.rs:122`). No
   link-introspection feature consumes it, so the partition stays transient and
   the READ-002 reshape can drop the restore; a future feature that wants it
   retained would add that deliberately.

## Coverage

- Read `AGENTS.md`, root `ARCHITECTURE.md`, `glass-lint-core/ARCHITECTURE.md`,
  and `TESTING.md`.
- Mapped the chunk: `analysis/project/{mod.rs,identities.rs,linker/{mod,export,graph}.rs,model.rs,projection.rs,projection/outcome.rs,projection/tests.rs,resolver.rs,state.rs}` (core files only; no `TESTING.md`/`CONTRIBUTING.md` changes needed for a read-only audit of behavior).
- Traced the linking flow: `ProjectSemanticModel::link_with_limits` ->
  `ProjectLinker::propagate_local_status` -> `collect_graph_edges`
  (`GraphBuild`, SCC partition) -> `resolve_export_table` / `validate_imported_exports`
  -> `finish` into `LinkedProjectState`; then projection through
  `ProjectionPlan`/`ProjectionSession`/`project_with_arena` into
  `ProjectionOutcome`, which feeds `record_analysis_status`
  (`lint/report/mod.rs:90-96,199-217`) and `lint/report/summary.rs:20-27`.
- Verified representative callers: `lint/report/mod.rs` (project/classification
  assembly), `lint/report/summary.rs` (metrics consumption), `analysis/tests.rs`
  (handle misuse test), `project/session/artifacts.rs` (`QualifiedRequestId`),
  `flow/cross` (`ExportLookupCache`), `api/compiler/requirements.rs` and
  `api/compiler/physical.rs` (flow requirement flags for READ-001),
  `lib.rs` (crate-private `analysis` module), `project/types/report/operations.rs`
  (aggregate operation counts), `glass-lint-harness/src/profile/metrics.rs`
  (profiling consumption), and `glass-lint-cli/src/output.rs` (summary output).
- Confirmed cross-crate (`js`, `project`, `obsidian`, `harness`) do not
  reference `ProjectMatcherModel`, `ProjectModuleHandle`,
  `project_for_classification`, or `assemble_classification_results`.
- No source, test, configuration, or documentation files were modified in this
  audit; only this audit file was created and polished. Other parallel chunk
  audits (`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_*.md`) were present and left
  untouched.
