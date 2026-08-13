# Codebase Readability Audit — Chunk 07

## Summary

Chunk 07 owns project linking: validated module/resolution input, the
deterministic module graph and SCC partition, bounded export fixed-point
resolution, shared export lookup, project identity overlays, and matcher
projection. The phase boundaries and conservative export states are sound.
The findings below target resource ownership that is currently communicated
through unrelated integer limits, temporary collection work around borrow
boundaries, and duplicated state representations at graph/result boundaries.

## Findings

### Project-linking state and resource ownership

#### [x] READ-029 — Status propagation materializes module IDs only to reacquire the modules

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/project/linker/mod.rs:98-123`

`ProjectLinker::propagate_local_status` first copies every key from
`self.modules` into a `Vec<ModuleId>`, then performs a second map lookup for
each ID before extending `self.status`. The method never mutates the module
map; the temporary ID list exists only to work around borrowing the status
field while reading another field of the same linker. This adds a full
project-sized allocation and lookup pass before every link without changing
the status values.

**Recommendation:** Iterate over the module values directly, or split the
disjoint module/status borrows locally so status propagation has one map
walk. Preserve deterministic `BTreeMap` order, per-file materialized status,
and the separate unsupported-interface diagnostic for unknown CommonJS
interfaces; only the ID vector and re-lookup should disappear.

**Fix Applied:** Propagation now walks linker module values directly with
disjoint module/status borrows, removing the intermediate ID vector and
re-lookup pass while preserving deterministic order and diagnostics. Verified
with `make fmt && make ci`.

#### [x] READ-030 — Export lookup-cache capacity is supplied by unrelated phase limits

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/state.rs:298-347`; `glass-lint-core/src/analysis/project/linker/mod.rs:73-91`; `glass-lint-core/src/analysis/project/projection.rs:99-120`; `glass-lint-core/src/analysis/project/model.rs:283-295`

`LinkingSession` owns only an `ExportLookupCache`, but its constructor accepts
an unnamed `capacity: usize`. The transient linker passes the project
`link_operations` limit, while `ProjectionSession` initializes the same
session with `project.flow_limit()`. A flow budget therefore changes the
retention and hit rate of an export-identity cache, while the cache API gives
no indication which resource it is bounding. The lookup remains conservative,
but projection can repeat bounded export walks or retain a cache sized for an
unrelated phase.

**Recommendation:** Give the cache a named export-lookup capacity owned by
the project-linking configuration, and carry that value into the immutable
project model when projection creates a new lookup session. The constructor
should accept that named policy, not a generic phase limit. Keep cache
insertion bounded, cached `None` distinct from a miss, recursive export-depth
checks, and the existing link/flow operation accounting independent.

**Fix Applied:** Added the named `MAX_EXPORT_LOOKUP_ENTRIES` policy and carried
its capacity into `ProjectSemanticModel`, linker sessions, and projection-created
lookup sessions. Link and flow operation limits no longer size this cache.
Verified with `make fmt && make ci`.

### Projection planning

#### [x] READ-031 — Projection planning scans every physical-root list twice

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:206-237`

`ProjectionPlan::from_selection` walks each matcher’s physical roots once to
collect non-empty constrained roots and then walks the same roots again with
`enumerate` to collect lifecycle roots. The two passes read the same immutable
plan data and differ only in the root variant and the lifecycle index they
retain. As catalogs grow, plan construction repeats this traversal for every
selected matcher and makes the root-to-projection classification policy live
in two adjacent loops.

**Recommendation:** Use one enumerated root pass that handles constrained and
lifecycle variants, then merge matcher requirements once as today. Preserve
the physical-root index used by `BoundLifecycleRoot`, the exclusion of empty
argument groups, rule ordering, and requirement aggregation; only the second
root traversal should be removed.

**Fix Applied:** Combined constrained-root and lifecycle-root collection into one
enumerated physical-root pass per matcher, preserving empty-group filtering and
the original lifecycle root indices. Verified with `make fmt && make ci`.

### Graph and fixed-point result boundaries

#### [ ] READ-032 — `GraphBuild` carries an error payload that production immediately discards

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/project/linker/graph.rs:21-31,33-114`; `glass-lint-core/src/analysis/project/linker/mod.rs:130-140`

`GraphBuild` stores `scc_partition` as
`Result<SccPartition, GraphBuildError>`, but
`ProjectLinker::collect_graph_edges` matches
`Err(_error)` and retains only `SccPartitionState::Rejected`. The same failure
already marks `GraphBuild.status` incomplete and sets `exhausted`; the sole
error variant carries no additional production information. The result error
therefore duplicates the rejected state and is used only by a unit test.

**Recommendation:** Make the graph builder return the partition as an
`Option`/domain state consumed by the linker; the existing status entry and
`exhausted` flag already carry the only production-relevant failure data. If a
typed error is retained for tests, keep it at the builder boundary rather than
wrapping it in the production result. Retain the explicit oversized-SCC
diagnostic, `exhausted` propagation, and fail-closed rejection of export
resolution.

**Fix Applied:** None so far.

#### [ ] READ-033 — Export-table updates clone a qualified name whose owner is already consuming it

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/project/state.rs:216-273`; `glass-lint-core/src/analysis/project/linker/export.rs:113-145`

`ExportTable::set_resolution` receives `&QualifiedExportId`, compares the
borrowed name, and then clones `id.name` to insert it into the module’s map.
`try_set_export` constructs the qualified ID for that call and does not use it
afterward; the cycle-exhaustion path likewise creates an ID only for the
update. Export fixed-point rounds can therefore clone every changed export
name even though the table is the owner that can consume the key.

**Recommendation:** Make the mutating operation consume `QualifiedExportId`
or accept the module and owned name separately, allowing the map entry to
move the name after the unchanged comparison. Preserve `Unchanged`,
`Inserted`, and `Replaced` outcomes, total-entry accounting, and the borrowed
lookup API used by recursive resolution; only the update-time name clone
should disappear.

**Fix Applied:** None so far.

#### [ ] READ-034 — Inserting a graph edge performs two map-entry lookups

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/project/state.rs:19-30`; `glass-lint-core/src/analysis/project/linker/graph.rs:41-62`

`ModuleGraph::insert_edge` calls `ensure_node(from)` and then immediately
calls `self.forward.entry(from).or_default()` again to append the target.
`GraphBuild` invokes this operation for each admitted internal edge, so the
normal edge path performs two ordered-map entry operations for one mutation.
`ensure_node` is still needed independently for modules with no outgoing
edges, but it need not be part of the edge insertion path.

**Recommendation:** Let `insert_edge` obtain the entry once and append the
target, while retaining the separate `ensure_node` call for isolated module
nodes. Preserve duplicate-edge normalization, deterministic neighbor order,
and the edge-budget admission check; remove only the redundant lookup.

**Fix Applied:** None so far.

## Systemic Themes

- A bounded resource must have one named owner. Export lookup-cache capacity,
  link operations, and flow operations currently meet through an untyped
  integer at the projection boundary even though they govern different
  lifecycles.
- Borrow-checker workarounds that allocate IDs or check tuples should be
  revisited at the owning type boundary. Disjoint field borrows or consuming
  mutators can preserve the phase invariant without project-sized temporary
  collections.
- Graph construction already has deterministic normalization and
  fail-closed status propagation. Its result types should not duplicate the
  same rejection state unless the error carries information a caller uses.
- Projection planning is a catalog-to-execution boundary; one traversal
  should classify each physical root while retaining the root index required
  for lifecycle correlation.

## Open Questions

- No semantic reason is present for projection to derive export-cache size
  from flow operations; use one named export-cache policy for both linking and
  projection unless a future measured deployment explicitly needs separate
  capacities.
- The status entry plus rejected state are the canonical production failure
  representation. A typed graph error may remain test-local, but must not be
  carried and discarded by production linking.
- All current `ExportTable::set_resolution` callers are internal update paths;
  the consuming update should preserve the borrowed lookup API and move only
  the update key.

## Coverage

Reviewed the chunk-07 structure entries and their implementation/test support:

- `analysis/project/{model,state,resolver,identities,projection}.rs`
- `analysis/project/linker/{mod,graph,export}.rs`
- Related analysis-limit, project-session, flow-cross, matcher-projection,
  and report callers were traced for resource ownership and lifecycle use.
- Existing numbered audit reports 001–006 were checked to avoid duplicating
  their completed historical findings.

No source, test, configuration, dependency, or other documentation files
were changed by this audit.
