# Codebase Readability Audit

## Summary

Chunk 7 owns the transition from validated local modules and resolver answers
to a bounded linked project model, then prepares project identities and flow
evidence for matching. The linker correctly separates local artifacts from
qualified overlays and uses deterministic SCC resolution, but several phase
contracts still collapse meaningful outcomes: oversized graph components are
represented as an empty partition without a status-bearing completion, query
helpers hide invalid-handle errors as empty evidence, and flow diagnostics use
aggregate projection work rather than flow-specific operations. The export
resolver also carries an unnecessary single-implementation trait boundary.

## Findings

### Graph construction and linker completion

#### [x] READ-025 — Preserve graph-limit completion in the linker state

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/linker/graph.rs:19-97`; `glass-lint-core/src/analysis/project/linker/mod.rs:34-46, 115-133`; `glass-lint-core/src/analysis/project/state.rs:46-95`

`GraphBuild::build` converts an oversized SCC into `SccPartition::default()`
and reports only a boolean `exhausted`. `ProjectLinker::collect_graph_edges`
marks an internal budget tracker, but `resolve_export_table` records the
linking `BudgetExhausted` status only when the partition has components; an
oversized graph therefore skips export resolution without adding the status
that makes the final project incomplete. The same empty/default state also
allows a linker with no graph phase to resemble a valid empty graph.

**Recommendation:** Return a typed graph-build outcome that distinguishes a
valid empty partition, edge-budget exhaustion, and SCC-size rejection, and
carry that outcome into `ProjectLinker`/`AnalysisStatus` before export
resolution. Retain the normalized graph for edge-count/diagnostic metadata,
but do not manufacture a default `SccPartition` when it is rejected; make the
export phase require a successful partition and make `finish` preserve the
incomplete state. Preserve deterministic SCC order, bounded fail-closed
identities, and the existing project-level linking diagnostic for both budget
causes.

**Fix Applied:** Replaced default SCC fallback with a typed `GraphBuildError`
and an explicit linker partition state that distinguishes pending, ready, and
rejected graph phases. Oversized SCCs now record the project linking-budget
status before export resolution, retain normalized graph metadata, and skip
unproved export resolution and import validation. A 4,097-module cycle
regression covers the typed rejection and stable project diagnostic.

### Project matcher query error surface

#### [x] READ-026 — Keep invalid project matcher queries fallible

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:400-406, 677-720`; `glass-lint-core/src/analysis/project/model.rs:455-477`

`ProjectMatcherModel::evidence_for_checked` already distinguishes foreign
handles, unselected rules, unknown rules, and unknown modules, but the
crate-visible `evidence_for` facade converts every error into an empty vector.
`ProjectSemanticModel::classify_with_evidence_limit` similarly calls
`CompiledRuleSelection::new(...).expect(...)` even though selection validation
has a typed error. Invalid project handles can consequently look like valid
no-match results, while invalid selections can panic rather than reaching the
caller’s error boundary.

**Recommendation:** Make the checked `Result` path the canonical project
query API and propagate a crate-private projection error through classification;
do not add a public error type while the analysis module remains private.
If a report assembler needs a lossy convenience, name it at that boundary and
document the intentional omission. Keep owner-token protection for foreign
models and the sorted/validated selection invariant; do not erase those
distinctions inside the linked model.

**Fix Applied:** Made the checked project evidence query the canonical
crate-internal API and retained an explicitly named lossy adapter only in
report assembly. Rule selection validation now returns its typed error from
classification; the report boundary records invalid selections as a
`rule_selection_invalid` diagnostic instead of panicking.

### Projection budgets and metrics

#### [ ] READ-027 — Account flow-budget observations separately from projection work

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:138-182, 475-514, 562-598`; flow sources `glass-lint-core/src/analysis/flow/projector/mod.rs:110-124, 967-984` and `glass-lint-core/src/analysis/flow/cross/mod.rs:52-58, 227-239`

`ProjectionOutcome.metrics.operations` aggregates overlay construction,
local flow, and cross-file flow operations. When flow is incomplete,
`ProjectionOutcome::finish` stores that aggregate as `flow_observed`, even
though the flow limit is charged by the local and cross flow owners and does
not include overlay work. A linking/report diagnostic can therefore report an
observed count that is not the flow budget’s unit, and changing a non-flow
projection phase changes flow diagnostics.

**Recommendation:** Give `ProjectionStatus` a flow-specific accumulator fed
only by `LocalFlowProjectionOutcome::operations` and
`CrossProjectionOutcome::operations`; retain the aggregate metric separately
for profiling/report totals. Set `flow_observed` from the flow accumulator at
the same completion boundary that marks flow incomplete, while preserving
effect-specific accounting, bounded execution, and deterministic output.

**Fix Applied:** None so far.

### Export lookup boundary

#### [ ] READ-028 — Remove the unused dynamic lookup abstraction

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/resolver.rs:19-101`; construction sites `glass-lint-core/src/analysis/project/linker/mod.rs:49-59` and `glass-lint-core/src/analysis/project/model.rs:325-335`

`ProjectLookup` has one private implementation, `ProjectLookupView`, and
`ExportResolver` stores it as `&dyn ProjectLookup`. Both transient linking and
the final model construct the same raw-map view before invoking the resolver,
so the trait object adds a second vocabulary and dynamic-dispatch boundary
without an alternate lookup owner. The repeated construction also leaves
the shared project lookup contract distributed between callers and the
resolver.

**Recommendation:** Make `ProjectLookupView` the concrete lookup boundary
owned by `ExportResolver` (or use a concrete generic parameter if a test
double is genuinely required), remove the single-implementation trait object,
and centralize construction of the view in one helper. Keep the resolver
independent of `ProjectLinker` and `ProjectSemanticModel`, preserve the
module-existence check in `request_target`, and retain the shared cache and
bounded cycle/depth behavior.

**Fix Applied:** None so far.

## Systemic Themes

- Linker phases use `Option`, default collections, and booleans to represent
  both valid emptiness and rejected/incomplete work. Typed completion should
  remain attached to graph and export-resolution state until status is
  materialized.
- Project linking, matching projection, and reporting each own a different
  error or budget domain. Their outcomes should carry domain-specific counts
  and failures rather than relying on aggregate fields or lossy convenience
  methods.
- The local-artifact/project separation is sound. The useful simplification is
  to centralize the qualified lookup and phase transitions, not to merge local
  value arenas with project identities or collapse transient and final model
  lifetimes.

## Decisions

- Retain the normalized graph for stable edge-count and diagnostic metadata,
  but reject the SCC partition as an executable linking phase when the bound
  is exceeded. The linker must carry an explicit rejected/incomplete outcome;
  an empty partition is not a valid substitute.
- Keep matcher query errors crate-private while the owning analysis module is
  private. The checked path is still canonical internally, and a future public
  re-export can choose a public error type when an actual public API exists.
- `ProjectionMetrics::operations` is a total projection/profile count.
  `ProjectionStatus::flow_observed` is the separate flow-budget observation;
  it must be accumulated only from local and cross-flow owners.

## Coverage

Reviewed only Chunk 7, “Project linking,” from `CODEBASE_STRUCTURE_CORE.md`,
including validated link inputs, module/export identity resolution, graph and
SCC construction, bounded export caches, transient linker state, linked
project models, matcher projection plans, project handles, and projection
outcomes. Existing Chunk 1 through Chunk 6 audit history was used to continue
IDs at READ-025. No source, test, configuration, dependency, or other
documentation files were changed; this chunk audit file is the only new
artifact.
