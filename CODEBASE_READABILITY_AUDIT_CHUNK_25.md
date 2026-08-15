# Codebase Readability Audit — glass-lint-core Chunk 25: Project report types

## Summary

Chunk 25 owns the public report contract consumed by `glass-lint-output`,
`glass-lint-cli`, `glass-lint-harness`, and `glass-lint-project`:
`AnalysisReport`, `FileReport`, `Finding`, `EvidenceTraces`/`EvidenceTrace`/
`EvidenceStep`, `Diagnostic`/`AnalysisDiagnostic`/`DiagnosticCode`/
`DiagnosticKind`, `SourceLocation`, `MatchCertainty`, `ReportCompletion`, and
`AnalysisOperationCounts`. The surface is small, validated, and mostly
well-behaved: semantic newtypes (`DiagnosticCode`, `ProjectRelativePath`,
`SourceLocation`) hide representation; constructors enforce non-empty evidence
invariants; `AnalysisReport::combine` fails closed on schema/tool/path
mismatches; `ReportCompletion` and `MatchCertainty` correctly model the
complete-vs-partial and definite-vs-possible distinctions.

The audit found no unsafe or panicking paths in the chunk's report assembly
(the `expect`s on canonical literals and on internal merge states are guarded
by construction). The recurring issues are internal duplication and
inconsistent surfaces: trace metrics stored twice in `AnalysisReport`
(`FinalizedReportAggregate` vs `AnalysisOperationCounts`), two different
canonical diagnostic orderings depending on construction path, a flag-routed
`EvidenceTraceState` enum that adds no vocabulary, a one-call-site
`ReportPathMetrics` DTO that exists to dodge a six-argument call, one dead
`ordering_key`, and minor code-generation conventions that bypass the
`DiagnosticKind` vocabulary.

## Findings

### report/analysis_report.rs — report aggregation and summaries

#### [x] READ-001 — Trace metrics are stored twice in `AnalysisReport`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/types/report/analysis_report.rs:183-240` and `glass-lint-core/src/lint/report/summary.rs:16-29`

`FinalizedReportAggregate` stores `evidence_steps` and `rendered_traces`
(computed in `from_parts`, analysis_report.rs:191-227) while the sibling
`AnalysisOperationCounts` (operations.rs:10,16) carries the same two values as
`evidence` and `rendered_traces`. The two stores are wired together by hand in
`assemble_project_report`: `operations.record_evidence(aggregate.evidence_steps())`
(summary.rs:19) and `rendered_traces: aggregate.rendered_traces()`
(summary.rs:28). Every assembled report therefore holds the identical pair in
two places, recomputed by different passes (`from_parts` rescans findings;
`finalize` recomputes the aggregate on every `into_partial`/`with_project_diagnostics`/
`combine`). The values can silently drift if one caller updates only one side.

**Recommendation:** Make `AnalysisOperationCounts.evidence`/`rendered_traces`
the single owner of these metrics (they are the externally serialized schema)
and stop storing them in `FinalizedReportAggregate`; return them alongside the
summary from one scan and record them once at assembly. Guardrails: keep the
serialized `evidence` and `rendered_traces` fields in `AnalysisOperationCounts`
unchanged (stable schema), keep `summary()` an O(1) accessor, and keep
`max_live_alternatives` merge semantics (max, not sum) untouched.

**Fix Applied:** Removed `evidence_steps`/`rendered_traces` from `FinalizedReportAggregate`; `from_parts` now returns them alongside the summary from one scan via `AnalysisReport::aggregate_and_evidence`, and `assemble_project_report` records them once into `AnalysisOperationCounts` (the serialized owner).

#### [ ] READ-002 — Two canonical diagnostic orderings, one per construction path

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/lint/report/summary.rs:15` vs `glass-lint-core/src/project/types/report/analysis_report.rs:129-136`

`assemble_project_report` sorts report-level diagnostics by `code()` only
(summary.rs:15), then builds the report without finalizing. `AnalysisReport::finalize`
sorts the same list by `Diagnostic::ordering_key()` — `(path, code, message)`
(diagnostic.rs:121-123, analysis_report.rs:133). A report returned directly
(`lint_source`, `ProjectAnalysis::into_report`) therefore uses the code-only
order, while any report that later passes through `into_partial`,
`with_project_diagnostics`, or `AnalysisReport::combine` is re-sorted into the
`(path, code, message)` order. The canonical ordering is defined in two places
with two keys, so equal-code reports order differently by path/message
depending on which assembly path produced them.

**Recommendation:** Make `AnalysisReport::finalize` the single sorting owner:
have `assemble_project_report` route the report it builds through `finalize` and
drop the code-only sort at summary.rs:15. `finalize` already sorts diagnostics
by `Diagnostic::ordering_key()` — `(path, code, message)` (diagnostic.rs:121-123)
— and recomputes the aggregate after sorting, so direct assembly,
`into_partial`, `with_project_diagnostics`, and `combine` all settle on one
deterministic order. Guardrails: preserve determinism of the serialized report;
keep the aggregate computation after the final sort; `public_report_transformations_preserve_diagnostic_order`
keeps passing because all of its diagnostics have `path=None`, so
`(path, code, message)` reduces to the expected code order.

**Fix Applied:** None so far.

### report/evidence.rs — evidence invariants and state routing

#### [ ] READ-003 — `EvidenceTraceState` enum is flag-routing indirection that adds no vocabulary

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/project/types/report/evidence.rs:94-126`

`EvidenceTraces` stores a `truncated: bool` field, yet the constructor surface
funnels that bool through a private `EvidenceTraceState { Complete(Vec),
Truncated(Vec) }` enum that is immediately destructured back into
`(traces, truncated)` by `from_state`. The enum exists only to route the flag;
it carries no invariant, no lifecycle boundary, and no vocabulary. The public
`with_truncation(traces, truncated: bool)` (evidence.rs:104) forces callers to
know the flag semantics ("a truncated collection may be empty, a complete one
may not"), which `lint/report/evidence.rs:94-95` and the harness
(`glass-lint-harness/src/types/protocol.rs:326`) both pass without conveying
intent.

**Recommendation:** Drop `EvidenceTraceState` and `from_state`; the enum only
routes a bool. Keep `EvidenceTraces::new` (rejects empty, not truncated) and add
a constructor that names the truncated state and tolerates empty input, so each
invariant lives in the constructor body that `merge` and `fallback` select.
Guardrails: keep fail-closed behavior (`new` still rejects empty), keep the
`EmptyTraces` vs `EmptyTrace` error distinction, keep `EvidenceTraces::merge`
and `fallback` semantics identical, and update every `with_truncation` call
site in the same change (`lint/report/evidence.rs:95`,
`project/report/tests.rs:411`, `glass-lint-harness/src/types/protocol.rs:326`).

**Fix Applied:** None so far.

### report/operations.rs — operation-count accumulator surface

#### [ ] READ-004 — `ReportPathMetrics` is a one-call-site DTO that dodges a six-argument call

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/project/types/report/operations.rs:85-93,124-131` and `glass-lint-core/src/lint/report/summary.rs:22-29`

`AnalysisOperationCountsBuilder` exposes seven single-field setters
(`record_files`, `record_requests`, …), but the six "path metric" fields are
delivered as the `ReportPathMetrics` struct — bare `pub(crate)` fields created
at exactly one call site (summary.rs:22-29), immediately copied field-by-field
in `record_path_metrics`, then dropped. The struct adds no invariant, no
validation, and no lifecycle; it only groups six arguments for a single
transition, which is the too-many-arguments-dodging shape the skill flags. The
builder's surface is internally inconsistent: seven fields get individual
setters, six get a bundled DTO.

**Recommendation:** Match the existing single-field-setter style: add the six
path-metric fields as individual setters (`record_max_live_alternatives`,
`record_trace_nodes`, `record_trace_heads`, `record_coalescing_comparisons`,
`record_fixed_point_iterations`, `record_rendered_traces`) like
`record_files`/`record_evidence`, and delete `ReportPathMetrics`;
`assemble_project_report` (summary.rs:22-29) calls the setters directly. All 13
builder fields then share one pattern and the too-many-arguments DTO
disappears. Guardrails: keep the crate-private visibility, preserve the
max-merge for `max_live_alternatives` and saturating-add for the rest in
`AddAssign` (operations.rs:138-159), and keep `operation_counts()` in
`analysis/project/model.rs:449-463` working unchanged.

**Fix Applied:** None so far.

### report/code.rs — diagnostic code vocabulary

#### [ ] READ-005 — Canonical-code test table omits `EvidenceCapacityMismatch` and duplicates the enum

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Testing
- **Location:** `glass-lint-core/src/project/types/report/code.rs:8-32` and `glass-lint-core/src/project/types/report/code/tests.rs:4-26`

`DiagnosticKind` has 22 variants, but the test
`diagnostic_kind_table_contains_only_canonical_codes` enumerates 21, silently
omitting `EvidenceCapacityMismatch` (code.rs:11, still constructed in
`analysis/semantic/status.rs:288`). The test re-declares the enum as a manual
array, so it drifts when a variant is added or renamed and would not catch a
bad `as_str` entry for the omitted variant.

**Recommendation:** Declare the variant list once in a `macro_rules!` beside
`DiagnosticKind` and expand it into the enum and the test's iteration, so the
test covers every variant by construction and immediately picks up a newly
added variant (including the currently omitted `EvidenceCapacityMismatch`);
`as_str`'s match stays compiler-checked exhaustive, so adding a variant forces
its entry. Guardrails: keep asserting that every `as_str` value is a valid
`DiagnosticCode` under the same validation rules and that `TryFrom<&str>`
round-trips each code.

**Fix Applied:** None so far.

#### [ ] READ-006 — `into_partial` hard-codes a code string that bypasses the `DiagnosticKind` vocabulary

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/report/analysis_report.rs:151-162`

`AnalysisReport::into_partial` builds `DiagnosticCode::new("incomplete_project")
.expect(...)` inline, while every other production report code flows through the
`DiagnosticKind` -> `DiagnosticCode::from` canonical mapping
(`From<DiagnosticKind> for DiagnosticCode`, code.rs:100-104). The literal
string is the stable schema value but exists in only one place with no named
owner, so it can drift from the code vocabulary or be duplicated. (The other
raw `DiagnosticCode::new` literals in the workspace are test-only or
project-boundary codes, e.g. `glass-lint-project/src/loader.rs:418`.)

**Recommendation:** Add the code to the central vocabulary (a
`DiagnosticKind::IncompleteProject` variant with `as_str` entry, or a
documented `const` code next to `DiagnosticKind`) and construct it through the
same `From` conversion as the other codes. Guardrail: the emitted string must
remain exactly `"incomplete_project"` — the report schema is stable.

**Fix Applied:** None so far.

### report/location.rs, report/diagnostic.rs — accessors and ordering keys

#### [ ] READ-007 — Owned-range accessor `range_owned` duplicated across three sibling types

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/report/location.rs:25-27` and `glass-lint-core/src/project/types/input/resolution.rs:53-55,82-84`

`SourceLocation::range_owned`, `ResolutionRequestKey::range_owned`, and
`ResolutionRequest::range_owned` each implement `self.range.clone()` as a
public convenience, with `ResolutionRequest::range_owned` forwarding to the
key's. `SourceRange` is `Clone` but not `Copy`, so the accessor is just sugar
over `.range().clone()`. In production only `SourceLocation::range_owned` is
called (`glass-lint-harness/src/types/protocol.rs:199`);
`ResolutionRequestKey::range_owned` is called only from in-crate tests
(`project/session/artifacts/tests.rs:121`, `project/tests/mod.rs:93`), and
`ResolutionRequest::range_owned` (resolution.rs:82-84) has no callers at all.
The same operation is re-declared on three sibling types instead of being one
documented owned-range contract.

**Recommendation:** Keep `SourceLocation::range_owned` (the harness adapter
conversion at protocol.rs:199 genuinely needs an owned range) and delete the
two `resolution.rs` accessors, replacing their test call sites with
`.range().clone()`. Guardrails: keep the borrowing `range()` accessors and the
public serde shape unchanged; the harness's `AdapterSourceLocation` conversion
must keep producing an owned `SourceRange`.

**Fix Applied:** None so far.

#### [ ] READ-008 — `AnalysisDiagnostic::ordering_key` is dead and defines a third ordering-key shape

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/types/report/diagnostic.rs:38-46`

`AnalysisDiagnostic::ordering_key` (diagnostic.rs:38-46) has no callers: all
report sorting goes through `Diagnostic::ordering_key` (diagnostic.rs:121-123,
used at analysis_report.rs:133) and `FileReport::ordering_key`
(file_report.rs:51-53). It survives as `pub(crate)` dead code and defines a
third, inconsistent tuple shape `(code, path, range)` versus the live
`(path, code, message)` key, which invites future mis-sorting or parallel-key
drift.

**Recommendation:** Delete `AnalysisDiagnostic::ordering_key` so only
`Diagnostic::ordering_key` and `FileReport::ordering_key` remain as the report
ordering contract. Guardrails: do not change the sort keys of the two live
methods (report determinism depends on them) and do not reintroduce a third
ordering shape.

**Fix Applied:** None so far.

## Systemic Themes

- **Derived-and-stored metrics duplicated across owners.** The report stores
  the same derived counters in both `FinalizedReportAggregate` and
  `AnalysisOperationCounts` and re-computes them in separate passes
  (READ-001). Prefer a single serialized owner plus one scan.
- **Canonical ordering defined per construction path rather than once.**
  `assemble_project_report` and `AnalysisReport::finalize` each establish a
  deterministic diagnostic order with a different key (READ-002); ordering
  should be owned by the finalized report.
- **Flag/state pass-through indirection.** `EvidenceTraceState`
  (READ-003) and `ReportPathMetrics` (READ-004) are transient wrappers that
  transport, rather than enforce, state — the enum routes a bool and the DTO
  groups arguments for a single call.
- **Small vocabulary drift around `DiagnosticCode`.** The `incomplete_project`
  literal (READ-006) bypasses the `DiagnosticKind` table, and the canonical
  table test itself drifts from the enum (READ-005).
- **Owned-range clone sugar repeated across types** (READ-007) and one stale
  ordering key (READ-008) are the kind of leftover accessor surface that
  accumulates across module splits.

## Open Questions

- `EvidenceTraces::fallback` (evidence.rs:171-180) is a public method whose only
  caller is a harness test (`glass-lint-harness/src/runner/tests.rs:38`). Is it
  an intentional external contract for adapter-shape evidence, or leftover from
  an earlier fallback path that `lint/report/evidence.rs` now implements
  directly (`FindingRenderer::fallback_trace`)? **Resolved:** intentional, not
  leftover. `fallback` is the only public constructor that yields a valid,
  non-truncated `EvidenceTraces` from a bare location, which the harness test
  needs to build expected findings. `FindingRenderer::fallback_trace`
  (lint/report/evidence.rs:202-212) returns `Vec<EvidenceStep>` — a step-builder,
  not a whole-traces constructor — and `FindingGroup::into_evidence` synthesizes
  its own inline trace, so there is no overlapping core path to delete.
- `DiagnosticKind` variant names intentionally diverge from their stable code
  strings (`FactsBudgetExhausted` -> `semantic_budget_exhausted`,
  `FactCapacityExhausted` -> `semantic_fact_capacity_exhausted`, etc.). This is
  presumably to keep schema strings stable while internal naming evolves; it is
  centralized in `as_str`, so it was not treated as a finding. **Resolved:**
  confirmed — the mapping lives in the single `as_str` match (code.rs:35-60),
  the match is compiler-checked exhaustive, and `From<DiagnosticKind>`
  validates every string as a `DiagnosticCode`, so the divergence cannot
  silently corrupt the schema.
- The `FinalizedReportAggregate` is recomputed on every `into_partial`,
  `with_project_diagnostics`, and `combine` (each calls `finalize`). For very
  large combined report sets this is a repeated full scan of all files and
  findings; whether that matters for realistic sizes is unmeasured. Each
  `finalize` is a single linear pass over files and findings, and the number of
  `finalize` calls is bounded by the transformations applied (one per call,
  plus one at the end of `combine`), so the cost is not a hot loop — but the
  measurement question remains open.
- `Diagnostic` is serde-tagged with `tag = "kind"` while `DiagnosticKind`'s
  canonical code strings live inside the payloads; confirming the intended
  stable schema shape for external consumers is outside this chunk's audit.

## Coverage

Scanned modules: `glass-lint-core/src/project/types/report/{mod.rs,
analysis_report.rs, code.rs, diagnostic.rs, evidence.rs, file_report.rs,
finding.rs, location.rs, operations.rs}` plus unit-test siblings; the report
assembly path `glass-lint-core/src/lint/report/{mod.rs, summary.rs, evidence.rs,
diagnostics.rs, files.rs}`; `glass-lint-core/src/project/report/mod.rs` and its
tests; `glass-lint-core/src/project/session/mod.rs`, `project/mod.rs`,
`project/types/mod.rs`, and `project/types/input/resolution.rs`; and
representative external consumers `glass-lint-output/src/report/render.rs`,
`glass-lint-cli/src/output.rs`, `glass-lint-project/src/loader.rs`, and
`glass-lint-harness/src/types/protocol.rs` / `runner/tests.rs`.

Read-only audit; no source, test, configuration, Cargo, or documentation files
were modified. The only file written is this audit document.
