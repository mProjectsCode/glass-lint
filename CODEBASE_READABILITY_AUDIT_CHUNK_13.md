# Codebase Readability Audit — Chunk 13

## Summary

Chunk 13 owns the project-linking state machine, export resolution, bounded
lookup caches, and the projection plan that turns linked modules into matcher
evidence. The phase boundaries are generally clear: linking builds a graph and
export table, resolution preserves unknown results, and projection keeps
physical roots tied to their compiled flows. The remaining risks are mostly
ownership leaks: graph normalization, export accounting, and recursion guards
are enforced by caller convention, while projection APIs use positional or
pointer identity as hidden validity checks. Export-target conversion is also
duplicated across the linker and resolver.

The project lookup/model lifecycle, flow-limit ownership, cache construction,
matcher context, cross-projection orchestration, and evidence-session concerns
were cross-checked against earlier Chunk 3, Chunk 4, and Chunk 7 reports and
are not repeated here.

## Findings

### Project graph and export-table ownership

#### [ ] READ-056 — Seal graph normalization before SCC and neighbor queries

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Lifecycle
- **Location:** `glass-lint-core/src/analysis/project/state.rs:15-78`; callers in `analysis/project/linker/graph.rs:17-76` and state tests around `state.rs:342-388`

`ModuleGraph::insert_edge` deliberately admits duplicate edges and documents
that they are removed by a later `normalize` call. `neighbors`,
`edge_count`, and `scc_partition` can nevertheless be called on the raw graph;
the linker currently remembers to normalize before partitioning, and tests
repeat that convention. Thus deterministic neighbor iteration and the meaning
of the edge count depend on every caller performing a separate lifecycle
transition. `SccPartition` then stores component vectors and an order of raw
component indexes without a graph state proving that the partition came from a
normalized snapshot.

This spreads one invariant across graph construction, linking, tests, and any
future analysis consumer. A new caller can observe duplicate or insertion-order
neighbors, report a different edge count, or compute a partition before the
documented normalization step without an API-level indication that the graph is
not ready.

**Recommendation:** Give the graph an owning consuming transition to an
immutable `NormalizedModuleGraph`. Let SCC partitioning consume or borrow only
that sealed representation, and expose edge counts and neighbors from the
same state. Preserve deterministic module ordering, duplicate-edge
elimination, isolated nodes, the maximum SCC bound, and the current
oversized-component fallback.

**Fix Applied:** None so far.

#### [x] READ-057 — Make `ExportTable` the sole owner of export-entry admission

- **Severity:** High
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Invariant
- **Location:** `glass-lint-core/src/analysis/project/state.rs:170-250`

`ExportTable` owns `total_entries`, which is the resource-accounting basis for
bounded project export linking, but its nested `ModuleExports::insert` method is
`pub(in crate::analysis)`. Any analysis module can therefore mutate a module's
map without updating `ExportTable::total_entries`; the current production path
uses the method only from `ExportTable::set_resolution`, but that safety is
conventional rather than represented by the types. `ModuleExports` also has no
operation that can distinguish a replacement from first admission for the
outer budget owner.

The split makes the export table's length and budget semantics fragile during a
new linker phase, cache replay path, or test helper. A direct nested insertion
can make the table appear under budget while containing more entries than the
linking limit accounts for, or make replacement behavior diverge from the
fixed-point update path.

**Recommendation:** Make `ModuleExports::insert` private and expose mutation
only through `ExportTable::set_resolution`, returning an explicit
`Inserted`/`Replaced` outcome. Keep `total_entries` as the current admission
metric in that owner; a future budget can replace it without reopening nested
map mutation. Preserve replacement without recounting, deterministic export
lookup, provisional-to-final SCC updates, unknown replacement, and the
existing bounded failure behavior.

**Fix Applied:** `ModuleExports::insert` is now private, and
`ExportTable::set_resolution` returns the typed `ExportUpdate` outcome while
owning first-entry accounting. Linker callers preserve changed/unchanged
behavior without reopening nested map mutation. Verified with `make fmt && make ci`.

### Export resolution

#### [x] READ-058 — Own the export-recursion guard inside `ExportResolver`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/project/resolver.rs:105-190`; callers in `analysis/project/linker/export.rs:307-341` and resolver callers around `resolver.rs:78-101`

`ExportResolver::lookup_export` requires each caller to allocate and maintain a
mutable `BTreeSet<QualifiedExportId>`. The method itself inserts and removes
IDs, while `walk_star_exports` forwards the same set through recursive calls.
Top-level callers create fresh sets for each root, so the recursion lifecycle is
correct only if every caller knows when a lookup is a new root and passes an
empty set. The public-within-analysis method also permits a caller to supply a
nonempty or otherwise shared set, coupling cycle detection to external setup.

That leaks the depth/cycle invariant and makes the resolution API harder to
read: callers are managing an implementation detail of star-export traversal
rather than asking the resolver for one export identity. It also creates an
easy path to false cycle detection or accidental guard sharing when a new
validation phase is added.

**Recommendation:** Split the operation into a top-level `lookup_export(id)`
that creates a fresh guard and a private recursive `lookup_export_inner(id,
visiting)`. Keep the guard, insert/remove discipline, depth limit, cache, and
unknown-result behavior in the resolver. Preserve independent root lookups,
cycle termination, bounded star-export traversal, cached negative results, and
the distinction between unknown and ambiguous exports.

**Fix Applied:** Split export lookup into a fresh-guard public operation and
a private recursive helper. Linker and resolver callers now request one
export identity without allocating or threading `BTreeSet` guard state,
while cycle/depth handling, cache behavior, and unknown results remain owned
by `ExportResolver`.

#### [x] READ-059 — Centralize known linked-target conversion

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/project/resolver.rs:196-249`; duplicate callers and matching logic in `analysis/project/linker/export.rs:274-341`

`target_to_export_resolution` and
`linked_target_to_export_resolution` each map `External`, `Builtin`, and
`Internal` targets to the corresponding `ExportResolution`, and both map
unsupported linked targets to `Unknown` through the known-target helper. The
first function additionally handles an absent target using the authored
specifier, while linker export resolution has separate call sites that invoke
the known-target conversion directly. The semantic conversion is therefore
split between two match blocks and several callers.

This is more than cosmetic duplication: adding a new `LinkedModuleTarget`
variant or changing how a target becomes an export identity requires finding
and updating multiple conversion boundaries. The absent-target policy can stay
separate, but the known-target mapping should have one owner so linker export
construction and imported-identity resolution cannot drift.

**Recommendation:** Put the known-target conversion on `LinkedModuleTarget` or
behind one private project-resolution helper returning `ExportResolution`.
Retain a thin `target_to_export_resolution` wrapper for the authored-specifier
fallback, and delete the repeated target arms from linker/export code. Preserve
internal-request fallback to `Unknown`, external and builtin naming, qualified
internal identities, and fail-closed handling of missing/outside/unsupported
targets.

**Fix Applied:** Kept authored-specifier fallback in
`target_to_export_resolution` and routed every present target through the
single known-target conversion helper, removing duplicate External/Builtin/
Internal mapping arms. Verified with `make fmt && make ci`.

### Projection plan and public model boundary

#### [x] READ-060 — Bind planned lifecycle flows to their physical roots

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:87-185,219-250`; downstream tuple conversion in `projection.rs:104-130` and `analysis/flow/projector.rs`

`PlannedFlow` stores a borrowed `CompiledObjectFlow` and a separate
`PhysicalRootIndex(usize)`. `ProjectionPlan::from_selection` obtains both by
enumerating `matcher.physical_roots()`, and `flow_input` later converts them
back into `(RuleIndex, usize, &CompiledObjectFlow)` for the flow projector.
The compiler does not enforce that the positional index still identifies the
root that owns the borrowed flow; the association is maintained by matching
two parallel pieces of state and by the downstream tuple contract.

This makes a change to root filtering, root ordering, or plan construction a
cross-module correctness concern. A flow can remain semantically valid while
its evidence routing index silently points at another root, especially when a
matcher has multiple lifecycle roots or when constrained roots and flow roots
are planned in separate loops.

**Recommendation:** Carry a validated root handle/descriptor in `PlannedFlow`
that owns both the lifecycle flow and its physical identity, or make the flow
projector accept `PlannedFlow` and derive the index at the single planning
owner. Remove the free positional tuple conversion after callers migrate.
Preserve rule identity, physical-root ordering, multiple lifecycle roots,
deterministic evidence routing, and the existing flow-limit/exhaustion
semantics.

**Fix Applied:** Added a typed `PlannedLifecycleRoot` that is constructed
only from a lifecycle physical root and owns both the root index and compiled
flow. Projection now passes a typed `FlowProjectionRule` to the projector,
which performs its internal plan conversion, preserving root ordering and
rule/evidence routing without exposing a positional tuple contract.

#### [ ] READ-061 — Replace pointer identity with a model-owned evidence handle

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** External API
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:272-305,572-592`

`ProjectMatcherModel::evidence_for` accepts an arbitrary `&ProjectModule`,
indexes projections by `module.id()`, and then uses `std::ptr::eq` to decide
whether the borrowed module is the exact instance retained by the model. A
module from another semantic model with the same ID therefore produces an
empty result rather than a typed ownership error, and callers must understand
that pointer identity—not the public module ID—is the real validity contract.

The check protects against mixing evidence from different linked projects,
but that protection is hidden in a public query API with a sentinel-like empty
result. It makes model lifetime and provenance assumptions part of every caller
and is difficult to preserve if projections become owned, cloned, or loaded
through another boundary.

**Recommendation:** Expose an iterator of model-owned module handles and
accept those handles in `evidence_for`; keep the raw pointer-based method
crate-private. Return an explicit missing or foreign-module outcome where the
boundary is public instead of silently returning no evidence. Preserve
selected-rule filtering, deterministic deduplication/normalization, project
isolation, and the current behavior for modules with no projection.

**Fix Applied:** None so far.

## Systemic Themes

- Bounded project state is represented in ordinary mutable maps and vectors,
  while normalization, admission accounting, and recursion limits are still
  caller-owned lifecycle conventions.
- Positional indexes and pointer equality currently act as hidden ownership
  tokens in projection APIs. Semantic handles would make project isolation and
  evidence routing reviewable at the boundary.
- Export identity conversion should have one provider-neutral owner so new
  linked-target variants cannot produce inconsistent resolution outcomes.
- The earlier reports' findings remain applicable: project model/report-session
  mutability, flow-limit placement, matcher context, and cross-projection
  orchestration are intentionally not duplicated here.

## Decisions

- Graph normalization is an explicit consuming phase producing an immutable
  normalized graph. SCC partitioning, edge counts, and neighbor queries all
  consume that representation; queries do not silently normalize copies.
- `ExportTable::total_entries` remains the current admission metric. A future
  export budget may replace it, but nested map mutation stays private to the
  export owner and cannot bypass counting or replacement semantics.
- Projection exposes model-owned module handles through iteration, and
  evidence lookup accepts those handles. Pointer identity is not a public
  contract, and foreign/missing handles return an explicit outcome.

## Coverage

Reviewed all types listed in Chunk 13 of `CODEBASE_STRUCTURE_CORE.md`:

- Project linking/model: `LinkerLookup`, `ProjectLinker`, `ExportResolution`,
  `LinkedProjectState`, `ProjectSemanticModel`, `QualifiedFunctionId`,
  `QualifiedRequestId`, and `ResolvedLinkInput`.
- Projection: `PhysicalRootIndex`, `PlannedConstrainedRoot`, `PlannedFlow`,
  `ProjectMatcherModel`, `ProjectModuleProjection`, `ProjectionInputs`,
  `ProjectionMetrics`, `ProjectionOutcome`, `ProjectionPlan`, and
  `ProjectionStatus`.
- Resolution/state: `ExportResolver`, `ProjectLookup`, `ExportLookupCache`,
  `ExportLookupCacheResult`, `ExportTable`, `LinkingSession`, `ModuleExports`,
  `ModuleGraph`, `QualifiedExportId`, `SccPartition`, `FrozenFactTables`,
  `ResolutionKey`, `ResolvedValue`, `Resolver`, `ResolverCache`, and
  `ResolutionSeed`.

Representative linker graph/export callers, SCC tests, export lookup and star
resolution, target conversion, projection planning, flow projection, and
evidence queries were traced. Prior Chunk 3, Chunk 4, and Chunk 7 findings were
cross-checked and not duplicated. No source, test, configuration, dependency,
or existing audit files were changed.
