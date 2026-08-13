# Codebase Readability Audit

## Summary

Chunk 7 owns the project semantic boundary: validated linker input, internal
module graphs and SCC order, bounded export fixed points, shared export lookup,
qualified identity overlays, and projection into matcher-ready module views.
The separation between local artifacts and project overlays, conservative
unknown/ambiguous resolutions, SCC ordering, and checked matcher handles is
appropriate. Five current opportunities remain: the export lookup cache keeps
redundant cardinality state, projection repeatedly reformats the same roots,
status recording scans a keyed module table linearly, invalid local facts still
trigger discarded identity work, and a test-only single-module constructor
requires unused source arguments.

## Findings

### Export lookup cache ownership

#### [ ] READ-062 — Remove the redundant export-cache entry counter

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/project/state.rs:312-351`

`ExportLookupCache` stores entries in a `BTreeMap` and separately tracks
`count`. The only mutation path increments `count` exactly when it inserts a
new map key, and the only capacity check is equivalent to
`entries.len() >= capacity`; no caller observes a count distinct from the
map’s cardinality. The duplicate state makes the cache’s bound an invariant
that two fields must maintain and leaves replacement behavior coupled to a
counter that has no independent meaning.

**Recommendation:** Make the map cardinality the cache’s single capacity
source and delete `count`, using `entries.len()` for the bound. Keep the
cache’s current non-evicting, bounded behavior and its distinction between a
cached `None` resolution and a miss; preserve deterministic qualified keys and
the shared session lifetime. Retain the cache behind `LinkingSession` rather
than exposing its storage to resolver callers.

**Audit disposition (2026-08-13):** Confirmed. `entries.len()` is the existing
capacity invariant; no eviction or cached-`None` behavior changes are implied.

### Projection plan boundary

#### [ ] READ-063 — Store constrained roots in the matcher-ready form

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:89-91, 205-213, 222-246, 278-286`

`ProjectionPlan` stores each constrained root in `PlannedConstrainedRoot`, a
two-field wrapper containing only the rule index and physical-root reference.
`project_facts` then allocates a fresh `Vec<(usize, &PhysicalRoot)>` for every
project module by calling the wrapper’s one conversion method. The plan has no
other consumer of the wrapper representation, so the same root list is
reformatted and allocated once per module before the matcher can use it.

**Recommendation:** Make the projection plan own the exact constrained-root
input expected by `try_compute_constrained_evidence`, or give that input a
typed rule-index form and pass the plan’s borrowed slice directly. Delete
`PlannedConstrainedRoot::matcher_input` and the per-module collection while
preserving selected-rule indices, matcher root order, empty-constraint
filtering, evidence capacity, and the independent lifecycle flow roots.

**Audit disposition (2026-08-13):** Confirmed with a type-boundary refinement:
prefer a matcher-ready private slice/type over exposing a raw tuple as a new
public surface.

### Projection status lookup

#### [ ] READ-064 — Use the project’s keyed module lookup for exhausted effects

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Performance
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:424-441`; keyed owner in `glass-lint-core/src/analysis/project/model.rs:306-311`

`ProjectionStatus::record_analysis_status` retains each exhausted module as a
`ModuleId`, but resolves it with `project.modules().find(...)`. The project
model already owns a `BTreeMap<ModuleId, ProjectModule>` and provides
`ProjectSemanticModel::module`, so recording status for `E` exhausted modules
scans all `M` modules for every entry instead of using the keyed lookup. A
project in which many modules exhaust effect extraction therefore performs
quadratic status assembly work after the projection itself has finished.

**Recommendation:** Route this status path through the model’s keyed module
accessor and delete the iterator scan. Keep the existing per-file diagnostic
scope, effect limit, observed-operation value, and deterministic order of
`effect_exhausted_modules`; retain the `Option` guard for an impossible or
stale module ID rather than turning status reporting into a panic.

**Audit disposition (2026-08-13):** Confirmed. Keep the stale-ID `Option`
guard; the optimization is only the lookup path.

### Invalid local projection work

#### [ ] READ-065 — Gate project identity construction on local projectability

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:149-180, 262-278`; invalidity contract in `glass-lint-core/src/analysis/facts/stream.rs:128-143` and `glass-lint-core/src/analysis/semantic/mod.rs:453-469`

`ProjectionSession::project_modules` computes module identities and call-result
identities before calling `project_facts`. `project_facts` then returns an
empty evidence result when the fact stream is invalid or the unknown value is
absent. The invalid-analysis contract also disables the matcher index and
effects (the name-exhaustion test demonstrates this), so the identity maps and
any occurrence overlay built for that module cannot be consumed by local
matching. This is a phase-order mismatch: projectability is known at the
consumer gate, but expensive project work is performed before that gate.

**Recommendation:** Centralize the cheap local-projectability predicate and
use it before constructing module and call-result identity overlays; retain an
empty matcher artifact/projection for the module so project status and module
ordering remain intact. Keep effect-completion/status propagation explicit for
flow-required plans, and preserve fail-closed behavior, local diagnostics,
cross-module identity isolation, and the distinction between invalid analysis
and a successful empty match.

**Audit disposition (2026-08-13):** Confirmed. The early gate must still emit
an empty per-module projection and preserve separate effect-status reporting.

### Single-module test API

#### [ ] READ-066 — Remove unused source arguments from the single-project fixture

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Testing
- **Location:** `glass-lint-core/src/analysis/project/model.rs:254-284`; callers in `glass-lint-core/src/analysis/mod.rs:112-123, 183-199`

The `#[cfg(test)]` `ProjectSemanticModel::single` helper accepts a path and a
`LocatedSourceContext`, forwards both to `single_with_limits`, and that helper
ignores both parameters. Callers consequently construct a source context for
the wrapper and then construct the same context again inside the
`LocalArtifact` that the model actually retains. The unused arguments obscure
that the local artifact already owns the complete path and source attachment,
and the discarded context can repeat line-index construction in test setup.

**Recommendation:** Make the test constructor accept only the `LocalArtifact`
and limits needed to build the one-module model, deleting the ignored
parameters and the callers’ discarded context construction. Keep the real
artifact’s path/source context as the sole location owner, preserve the
single-module owner-token tests, and leave production linking APIs unchanged.

**Audit disposition (2026-08-13):** Confirmed. This removes discarded test
setup only and leaves production linking boundaries untouched.

## Systemic Themes

- Project phases should exchange the representation their next owner already
  consumes. Repeated wrapper-to-tuple conversion and pre-gate identity work
  make projection orchestration pay for state that is immediately discarded.
- Map-backed project state should use its key at the owner boundary. Parallel
  counters and linear scans weaken the bounded-state design without adding a
  semantic distinction.
- Invalid or incomplete local analysis must remain a valid project entry for
  diagnostics and ordering, but should not trigger matcher work that the
  capability contract has already disabled.
- Export fixed-point state, SCC graph normalization, ambiguity handling,
  qualified IDs, project/model handle ownership, and conservative resolution
  conversion were reviewed and retained as necessary architecture. The
  transient linker and immutable project model were not collapsed because
  their mutation and lifecycle boundaries are distinct.

## Open Questions

- None blocking these findings. READ-065 should be implemented with the
  existing effect-status path explicitly covered because projection status is
  reported separately from local matcher evidence.

## Coverage

Reviewed only Chunk 7, “Project linking,” from `CODEBASE_STRUCTURE_CORE.md`:
validated linker input and request-target normalization, module graphs and SCC
partitioning, export fixed-point resolution, qualified export lookup and
bounded caching, imported/namespace/call-result identities, project semantic
state, matcher projection plans and sessions, evidence handles, projection
status/metrics, and project/linking tests and callers. The root and core
architecture documents, testing/contribution guidance, current audit chain,
invalid-fact capability tests, cache tests, and project-flow integration tests
were inspected. Focused tests passed: `cargo test -p glass-lint-core
analysis::project --lib` (10 passed) and `cargo test -p glass-lint-core
project::tests --lib` (49 passed). No source, test, configuration, dependency,
or other documentation files were changed; this chunk audit file was updated
only with review dispositions. The next chunk is Chunk 8, “Rule authoring and catalog
integration,” which should continue finding IDs at READ-067.
