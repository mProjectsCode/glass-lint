# Codebase Readability Audit

## Summary

Chunk 11 owns linter construction, rule-selection resolution, bounded batch
execution, deterministic range reduction, and the link/match/render/finalize
report pipeline. The main phase transitions are clear: `Linter` freezes
validated shared state, batch execution limits input admission and restores
input order, and report stages consume one another by value. One ownership
opportunity remains in the finalized-report boundary: report aggregation is
computed as transient assembly state and then recomputed by the public report
accessor, while aggregate metrics and summary are represented as parallel
values rather than one retained finalized aggregate.

## Findings

### Finalized report aggregate ownership

#### [x] READ-041 — Retain finalized report summary data instead of recomputing it

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/report/summary.rs:13-47`; aggregate implementation `glass-lint-core/src/project/types/report/analysis_report.rs:138-229`; public accessor `glass-lint-core/src/project/types/report/analysis_report.rs:253-257`

`summary::assemble_project_report` computes a `FinalizedReportAggregate`
from all files and diagnostics, uses it to record evidence and rendered-trace
operation counts, and then discards it before constructing `AnalysisReport`.
The finalized report retains only the raw files, diagnostics, and operation
counters. Every later call to `AnalysisReport::summary()` traverses all files,
findings, evidence traces, and diagnostics again to recreate the summary.

The same finalized data therefore has two lifetimes and two owners: report
assembly computes aggregate metrics once, while the public report API rebuilds
summary data on demand. This makes a seemingly cheap accessor scale with the
entire report and leaves the stored operation counters and summary derived
from separate traversals. Adding a new summary field requires updating the
aggregate traversal and every assembly/accessor path that consumes its
different projections.

**Recommendation:** Retain a finalized aggregate or summary value inside
`AnalysisReport`, and have the assembly pipeline pass the aggregate it already
computed into the report constructor. Make controlled mutation helpers such
as `with_project_diagnostics` and `into_partial` refresh the same aggregate
once before finalization. Expose the retained summary by value while keeping
the current deterministic file/diagnostic ordering and operation-count
semantics.

**Fix Applied:** Retained `FinalizedReportAggregate` inside `AnalysisReport`
as non-serialized finalized state. Assembly passes its existing aggregate into
the report, `summary()` returns the retained value, and finalization refreshes
it after deterministic report mutations and merges.

## Systemic Themes

- Linter and batch configuration use validated, private state effectively;
  the remaining boundary weakness is concentrated in finalized report
  derived data rather than execution scheduling or selection semantics.
- Report assembly computes several derived views from the same immutable
  inputs. Those views should be sealed together once so public accessors do
  not repeat full-report traversals or maintain parallel metric vocabularies.
- Determinism is preserved by ordered maps and explicit sorting, so any
  aggregate consolidation should retain those existing order boundaries.

## Decisions

- Retain the finalized summary aggregate internally and expose it through the
  existing accessor, but do not add serialized summary fields. The report
  schema remains stable and summary is derived report metadata rather than a
  second wire-format contract.
- Keep `AnalysisOperationCounts` as an independent public performance snapshot.
  It measures analysis phases, while summary counts measure files/findings/
  diagnostics; one retained internal aggregate may own both projections, but
  the public meanings must stay separate.
- Keep `with_project_diagnostics` and `into_partial` as public consuming
  transformations because callers use them after assembly. They must refresh
  the retained aggregate exactly once after adding diagnostics and before
  returning the finalized report.

## Coverage

Reviewed only Chunk 11, “Lint execution and reporting,” from
`CODEBASE_STRUCTURE_CORE.md`, including linter configuration and immutable
shared state, catalog/selection integration, single-source and bounded batch
execution, input-order restoration and cancellation, range containment
reduction, project-analysis timing values, report link/match/render/finalize
transitions, diagnostics attachment, evidence grouping and fallback traces,
summary aggregation, completion state, and public report accessors. Existing
Chunk 1 through Chunk 10 audit history was used to continue IDs at READ-041.
No source, test, configuration, dependency, or other documentation files
were changed; this chunk audit file is the only new artifact for Chunk 11.
