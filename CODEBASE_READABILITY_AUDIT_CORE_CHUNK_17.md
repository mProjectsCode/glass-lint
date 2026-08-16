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
although nothing outside `glass-lint-core` consumes it; and `ProjectionMetrics`
carries a write-only `operations` counter with duplicated `record_*`
accumulation. Findings are ordered so that the owner consolidation (READ-002)
precedes the wiring deduplication that depends on it (READ-003).

## Findings

### Projection orchestration (`analysis/project/projection.rs`)

#### [ ] READ-001 — "Does this plan need flow" is computed twice from different sources

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:174,207-209,371,382`

`ProjectionPlan::needs_flow()` returns `!self.flow_matchers.is_empty()`
(projection.rs:207-209) and decides whether effect summaries are loaded per
module (projection.rs:174), while `project_with_arena` independently computes
`let has_flow = plan.requirements.flow().local() || plan.requirements.flow().cross_call()`
(projection.rs:371) to decide whether the cross-module pass runs
(projection.rs:382). The two flags are only set together by the lifecycle
root lowering (`api/compiler/physical.rs:164-165`), so the predicates are
equivalent today but are expressed through two different artifacts
(`flow_matchers` vs `PlanRequirements.flow`), letting the effect-loading path
and the cross-collection path drift apart without a single owner to keep them
in lockstep.

**Recommendation:** Keep one predicate on `ProjectionPlan` (the owner of the
lowered roots), e.g. make `needs_flow()` the solely authoritative check, and
delete the inline `requirements.flow()` computation in `project_with_arena`
so both the effect loading and the cross pass test the same boolean.
Guardrail: keep `requirements.flow().local()/cross_call()` intact for plan
preparation and profiling; the runtime gate must stay true whenever a
`PhysicalRoot::Lifecycle` was lowered for a selected matcher.

**Fix Applied:** None so far.

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
(linker/mod.rs:142-159). The retained project state is therefore owned by two
structs that must be kept field-for-field in sync whenever one changes,
and every future field that survives linking has to be added in two places.

**Recommendation:** Give the retained semaantic state a single owner:
have `ProjectLinker` hold (or wrap) a `LinkedProjectState` for the retained
fields while keeping only the transient items (`graph`, `scc_partition`,
`lookup_session`, budgets, `link_limit`) as linker-scoped fields, so
`finish()` moves the state out directly instead of copying it into a parallel
literal. Guardrails: the transient graph/SCC/cache/budget state must not
survive into the immutable model; `edge_count` (from `NormalizedModuleGraph`)
must still be captured at the handoff; the immutable, `Send + Sync`
post-link model contract stays intact.

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
`ProjectSemanticModel::resolve_imported_identity` (model.rs:321-335), with the
same `(modules, resolutions, exports, cache)` trio pulled from parallel field
sets on the two owners (which READ-002 currently forces into sync).
Beneath it, `ProjectLookupView` (resolver.rs:20-51) wraps the first two maps
only; its `module()`/`request_target()` methods are called exclusively from
within `ExportResolver`, so the view is a single-consumer intermediate rather
than a shared API.

**Recommendation:** Inline `ProjectLookupView` into `ExportResolver`
(deleting the view struct and its constructor), then give both owners one
shared call path into the lookup layer so the `from_maps(...)` wiring exists
once — e.g. a small `resolve_imported_identity`-style helper owned by the
retained project state from READ-002 that both the linker and the model call.
Guardrails: the linker must keep resolving through the same bounded
`ExportLookupCache` it already owns, and the two different fallback behaviors
(`target_to_export_resolution` with absent-target fallback vs
`linked_target_to_export_resolution`) must stay distinct.

**Fix Applied:** None so far.

### Projection public surface (`analysis/project/projection.rs`, `analysis/project/model.rs`)

#### [ ] READ-004 — Projection pipeline is published broader than its actual boundary

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:44,58,278,305,411,421-464`

`project_for_classification` (projection.rs:44), `assemble_classification_results`
(projection.rs:58), `ProjectMatcherModel` (projection.rs:278), and
`ProjectModuleHandle` (projection.rs:305) with its `modules()` iterator are all
`pub`, yet the only production caller of the pair of free functions is
`ProjectSemanticModel::classify_with_evidence_limit` (model.rs:454-477), which
consumes the model entirely inside `glass-lint-core`. The handle + owner-token
+ `EvidenceQueryError` machinery (projection.rs:284-301) is likewise only
exercised in-crate, primarily by the misuse test
`analysis/tests.rs:89-138`. Publishing these entry points and types larger
than the crate requires it exposes internal wavefront internals (atomic global
token, handle/owner identity, error variants) that no external crate can
legitimately use, inflating the surface the public API must stabilize.

**Recommendation:** Shrink the pipeline to `pub(crate)` (the free functions,
`ProjectMatcherModel`, `ProjectModuleHandle`, and their methods), keeping the
`Result`-returning `evidence_for` reachable for in-crate tests, and revisit
whether the global atomic `ProjectMatcherIdentity` + `ForeignModel` check is
needed once the type can no longer escape the crate. Guardrails: preserve the
misuse-detection contract asserted by `analysis/tests.rs:89-138` (querying a
model with a handle from another model must keep failing closed), and keep the
evidence-order/determinism contract of `evidence_for_checked`.

**Fix Applied:** None so far.

### Projection outcome accounting (`analysis/project/projection/outcome.rs`, `projection.rs`)

#### [ ] READ-005 — `ProjectionMetrics::operations` is write-only and `record_local`/`record_cross` duplicate the same accumulation

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/projection/outcome.rs:89-102,139-168`, `glass-lint-core/src/analysis/project/projection.rs:173`, `glass-lint-core/src/analysis/project/projection/tests.rs:15`

`ProjectionMetrics::operations` (outcome.rs:101) is incremented in three
places (projection.rs:173 for overlay work, outcome.rs:157 and 167 in
`record_local`/`record_cross`) and is never read by any production code;
`lint/report/summary.rs:20-27` only reads the five narrower fields. `record_local`
(outcome.rs:139-158) and `record_cross` (outcome.rs:160-168) also repeat the
same three-step flow aggregation (`mark flow incomplete when exhausted`,
`flow_operations +=`, `trace_heads +=`, `operations +=`). This is
leftover accounting that adds hot-path accumulation for a value nothing
consumes, forcing reviewers to distinguish it from the `flow_operations`/
`flow_observed` counters that genuinely feed budget diagnostics.

**Recommendation:** Delete the write-only `ProjectionMetrics::operations`
field and its three accounting sites (including the vestigial
`metrics.operations = 100` set in projection/tests.rs:15), unless profiling
intends to surface a total-projection-operation count, in which case add a
getter and wire it into report operations instead. Extract the shared flow
accumulation (incomplete marking + `flow_operations`/`trace_heads` deltas)
into one private helper used by both `record_local` and `record_cross`.
Guardrails: keep `flow_observed`/`effect_observed`/`evidence_error`
reporting semantics and the `flow_operations`-excludes-overlay-ops
distinction asserted in projection/tests.rs:3-20.

**Fix Applied:** None so far.

### Export identity conversions (`analysis/project/linker/export.rs`, `analysis/project/identities.rs`)

#### [ ] READ-006 — Static-string-vs-qualified `ExportResolution` construction is repeated

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/linker/export.rs:240-248,262-268` and `glass-lint-core/src/analysis/project/identities.rs:85-92`

`resolve_local_export` (export.rs:240-248) and `resolve_value_export`
(export.rs:262-268) each map a module interface's `static_string(name)` to
`ExportResolution::StaticString`, falling back to `Qualified { module,
export: name }` with the identical `map_or_else` shape. A third variant
(identities.rs:85-92) builds `StaticString` from a returned static value with
an `Unknown` fallback. The same low-level "interface static string to
`ExportResolution`, with a caller-chosen fallback" conversion is therefore
spelled out three times.

**Recommendation:** Extract one private constructor on the narrowest shared
owner (e.g. `ExportResolution` in `analysis/project/model.rs` or a helper in
`resolver.rs`) that takes the optional static string plus a fallback
resolution, and call it from all three sites so the `map_or_else` shape and
`to_owned` handling exist once. Guardrails: keep the `Qualified` fallback for
the two export.rs paths and the `Unknown` fallback for the effect-return path,
and preserve the Local-provenance precedence (static string wins over
qualified).

**Fix Applied:** None so far.

## Systemic Themes

- **Two-phase lifecycle re-encoding.** The transient-to-immutable boundary
  (RESOLVED `ResolvedLinkInput` -> `ProjectLinker` -> `ProjectSemanticModel`)
  is sound, but the retained fields are re-listed into `LinkedProjectState`
  (READ-002), and the lookup wiring built over that trio is re-constructed at
  the other owner (READ-003). Each new surviving field currently has to appear
  in two structs and one handoff literal.
- **Predicates recomputed at different layers.** The same boolean ("plan runs
  flow") and the same conversion (static string vs qualified identity) are
  computed from two different sources of truth (READ-001, READ-006).
- **Public surface exceeds crate boundary.** The entire projection query
  pipeline (model, handle, identity token, error enum) is `pub` while all
  progammatic consumers live inside `glass-lint-core` (READ-004).
- **Write-only accounting.** `ProjectionMetrics::operations` is accumulated in
  the hot path and never read; flanking `record_*` methods duplicate the same
  aggregation shape (READ-005).

## Open Questions

- Is a cross-crate consumer (e.g. `glass-lint-harness` verification or
  profiling) planned for `ProjectMatcherModel`/`ProjectModuleHandle` that
  would justify keeping them `pub`, including the handle owner-identity
  contract? If so READ-004 should be rescoped rather than applied.
- Does profiling intend to surface a total projection-operation counter? If
  yes, `ProjectionMetrics::operations` needs a getter and report wiring rather
  than deletion (part of READ-005).
- `QualifiedRequestId` is deliberately `pub` with a `pub(crate)` constructor
  and no getters ("fields are never decomposed"), while the sibling tokens
  `QualifiedFunctionId` and `QualifiedExportId` are `pub(in crate::analysis)`
  with `const` constructors and getters. The asymmetry is justified by
  `project/session/artifacts.rs` crossing the `analysis` boundary; confirm the
  no-decomposition contract is part of the public API and worth documenting
  on the sibling types too.
- `SccPartition` is reconstructed as `Option` and restored with a take/put-back
  dance in `linker/export.rs:42-59` and then dropped entirely at `finish()`.
  It is only needed during linking, so there is no invariant leak, but confirm
  future link-introspection features would want it retained before reshaping
  the transient/final state split.

## Coverage

- Read `AGENTS.md`, root `ARCHITECTURE.md`, `glass-lint-core/ARCHITECTURE.md`.
- Mapped the chunk: `analysis/project/{mod.rs,identities.rs,linker/{mod,export,graph}.rs,model.rs,projection.rs,projection/outcome.rs,resolver.rs,state.rs}` (core files only; no `TESTING.md`/`CONTRIBUTING.md` changes needed for a read-only audit of behavior).
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
  `api/compiler/physical.rs` (flow requirement flags for READ-001).
- Confirmed cross-crate (`js`, `project`, `obsidian`, `harness`) do not
  reference `ProjectMatcherModel`, `ProjectModuleHandle`,
  `project_for_classification`, or `assemble_classification_results`.
- No source, test, configuration, or documentation files were modified in this
  audit; only this audit file was created. Other parallel chunk audits
  (`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_*.md`) were present and left
  untouched.