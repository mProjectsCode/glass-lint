# Codebase Readability Audit

## Summary

Chunk 3 owns bounded local object flow, function effects and summaries, the
cross-call overlay, flow completion, and evidence emission. The canonical fact
tape, correlated path state, typed lifecycle roots, and shared trace-chain
construction are appropriate boundaries; the historical Chunk 3 findings were
rechecked and not re-reported. Three current opportunities remain: target
indexes discard matcher-bearing declarations and force repeated scans, summary
completion is lost before local certainty is finalized, and summary lifecycle
work is named as a `FlowCompletion` operation even though it mutates summary
state.

## Findings

### Bound flow target indexes

#### [ ] READ-050 — Keep matcher-bearing declarations in bound target indexes

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Performance
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:161-184, 217-279, 328-400`; cross-source binding in `glass-lint-core/src/analysis/flow/cross/sources.rs:204-216, 232-285`; local source matching in `glass-lint-core/src/analysis/flow/projector/transfer.rs:55-75`; sink rescans in `glass-lint-core/src/analysis/flow/summary/sink.rs:218-279` and `glass-lint-core/src/analysis/flow/projector/evidence.rs:68-117`

The shared planning boundary already binds source declarations into
`BoundSource` values that retain argument constraints, and local projection
uses those values directly. Cross-flow source collection instead rebuilds a
`BoundTargetIndex<FlowId>` with `|id, _| id`, then reopens every candidate
`CompiledObjectFlow` and scans `flow.sources()` to rerun target and argument
matching for each call. The sink index makes the same representation tradeoff:
it stores only `FlowId`, so local flow, cross flow, and summary collection scan
the selected flow's sinks again to recheck target shape and argument indexes.

**Recommendation:** Make the bound target indexes retain the matcher-bearing
declaration selected by the target key: reuse `BoundSource` in cross-flow
source discovery and add a private bound-sink entry carrying its `FlowId`,
sink index, and argument matcher state. Expose narrow planning methods that
return already-bound matching entries, then delete the cross `SourceIndex`
adapter, `flow.sources().any(...)` scan, and repeated sink rescans. Preserve
rooted-member versus global-call semantics, per-module name binding, argument
matcher behavior, deterministic entry ordering, and the distinction between
source candidates and sink completions; do not merge these into one generic
flow declaration type merely to remove the loops.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. The index should retain the
already-bound matcher state, but source and sink declarations remain distinct
because they complete different flow phases.

### Function-summary completion boundary

#### [x] READ-051 — Carry full summary completion into local flow outcomes

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary/summaries.rs:20-100, 103-105`; `glass-lint-core/src/analysis/flow/projector/mod.rs:75-101, 137-167, 515-545, 938-958`

`FunctionSummaries` records a rich `FlowCompletion` containing summary,
summary-budget, sink-capacity, and worklist-capacity reasons, but its result is
not exposed after collection. `collect_into` passes only the unrelated
`summary_budget.exhausted()` boolean into `ProjectionRunState`; consequently,
summary sink/worklist exhaustion (and other summary completion reasons) can be
lost before `FlowCompletion::from_sources` decides whether retained local
evidence must be downgraded to `Possible`. This splits the incomplete-analysis
contract across a rich summary field and a narrower flag and can make a
partially propagated helper summary look complete to local emission.

**Recommendation:** Make `FunctionSummaries` expose or consume a single
summary outcome carrying its `FlowCompletion`, and merge that outcome into the
projector's completion before evidence certainty is finalized; remove the
`summary_exhausted: bool` transport once the typed completion is used. Move the
summary-sink sorting currently implemented as `FlowCompletion::finalize` onto
`FunctionSummaries` (or its consuming finish transition), since it mutates
summary state rather than completion state. Preserve retained independent
complete witnesses as `Possible`, prevent incomplete alternatives from
establishing `Definite`, retain every completion reason deterministically, and
keep the existing bounded summary storage.

**Fix Applied:** `FunctionSummaries` now exposes its typed completion to the
local projector, which merges every summary completion reason into the final
flow outcome instead of transporting only a summary-budget boolean. Summary
sink sorting now belongs to `FunctionSummaries::finalize`, and propagation
completion is merged without discarding existing reasons. Added a focused
regression test for preserving `SummaryBudget`. Verified with `make fmt && make
ci`.

**Audit disposition (2026-08-13):** Confirmed and prioritized. This is the root
completion-boundary issue: all summary completion reasons must reach certainty
classification before evidence is emitted.

### Summary-path and effect-facing APIs

#### [x] READ-052 — Remove the summary finalization operation from `FlowCompletion`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/summary/summaries.rs:57-63, 91-100`

The inherent `FlowCompletion::finalize` function accepts a mutable
`FunctionSummaries` and only sorts each summary's sink set. It neither reads
nor updates the `FlowCompletion` value, so the current owner name makes the
summary lifecycle look like a completion-state transition and obscures where
the summary's deterministic ordering invariant is established. The misplaced
method also contributed to the completion boundary being easy to overlook in
`FunctionSummaries::collect`.

**Recommendation:** Treat this as part of READ-051, not as a separate refactor.
When the summary outcome is consumed, move the operation to a private
`FunctionSummaries::finalize`/`finish` transition and call it from collection;
delete the `impl FlowCompletion` block for this behavior. Keep sorting after
all direct and propagated sink insertion, preserve the existing flow/sink/path
ordering, and keep completion reason merging separate from summary mutation.

**Fix Applied:** Covered by READ-051: summary sorting moved to the private
`FunctionSummaries::finalize` transition. Verified with `make fmt && make ci`.

**Audit disposition (2026-08-13):** Subsumed by READ-051. Implementing this as
an independent change would split one completion-boundary fix and add needless
churn; it remains a concrete acceptance criterion for READ-051.

## Systemic Themes

- Bound planning should retain semantic matcher state at the physical lookup
  boundary. Reopening compiled declarations in local, cross, and summary
  consumers creates repeated work and makes target-shape policy evolve in
  parallel.
- Completion is a domain result, not a boolean side channel. Every bounded
  phase must carry its reasons to the evidence owner before certainty is
  decided, while independent complete witnesses remain available as possible
  findings.
- Lifecycle operations should live on the type whose invariant they mutate;
  completion flags should classify outcomes rather than orchestrate summary
  storage.
- The two-pass local projector, correlated checkpoint history, bounded
  worklists, and shared trace arena remain necessary and were not treated as
  simplification targets.

## Open Questions

- None blocking these findings. Historical READ-008 and READ-009 were
  revalidated as applied. Historical READ-010 was closed as intentionally
  separate local/cross evidence ownership, and READ-011 was applied through
  the shared `intern_lifecycle_trace` boundary; neither is re-reported.

## Coverage

Reviewed only Chunk 3, “Flow analysis,” from `CODEBASE_STRUCTURE_CORE.md`,
including function-effect extraction, bound flow planning, local projector
state/control/loops/history, summary paths and propagation, cross-module source
and context worklists, cross evidence, completion outcomes, and local/cross
trace emission. Current callers, focused flow tests, architecture guidance,
and the historical Chunk 3 audit and applying commits were inspected. No
source, test, configuration, dependency, or other documentation files were
changed; this chunk audit file was updated only with review dispositions. The next chunk is
Chunk 4, “Retained models/resolution,” which should continue finding IDs at
READ-053.
