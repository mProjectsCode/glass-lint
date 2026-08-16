# Codebase Readability Audit

## Summary

This audit covers Chunk 25 ("Project report types") of `glass-lint-core`: the
public report value family in
`glass-lint-core/src/project/types/report/**` plus the re-export surface in
`project/types/mod.rs`. It is read-only; no source was modified. Only
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_25.md` was created; the pre-existing
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_*.md` files from parallel sessions were
left untouched.

The report family is well-behaved at the boundary: `AnalysisReport`,
`FileReport`, `Finding`, `EvidenceTraces`, and `SourceLocation` expose
private-storage accessors only; evidence non-empty shapes are enforced at
construction (`EvidenceConstructionError`); `DiagnosticCode` is a
validated newtype with a hand-maintained `DiagnosticKind` factory guarded by a
table test; `MatchCertainty` (Definite/Possible) with a Definite-stronger merge
matches the architecture note; and `AnalysisReport::finalize` plus
`EvidenceTraces::merge` make ordering deterministic. Highlight locations
`SourceLocation::new` receives already-validated `ProjectRelativePath` and
`SourceRange` components, so no new validation body was required there.

The findings below concentrate on the finalized-report seam in
`analysis_report.rs`, the write-side proxy in `operations.rs`, one
whole-state mutator on a public diagnostic DTO, and repeated
variant-classification predicates between `FileReport` and the summary scan.

## Findings

### Finalized report aggregation

#### [ ] READ-001 — `FinalizedReportAggregate` is a one-field wrapper whose value is always immediately computed twice

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/types/report/analysis_report.rs:43-44,69-87,130-137,163-177,191-240`, `glass-lint-core/src/lint/report/summary.rs:15-42`

`FinalizedReportAggregate` stores only `summary: AnalysisReportSummary` and adds
no invariant, vocabulary, or behavior beyond delegating `from_parts`
(analysis_report.rs:200-235); it is never re-exported, so it is effectively
crate-private. On the production path the summary is computed twice per
assembly: `assemble_project_report` runs one full scan for
`aggregate_and_evidence` (summary.rs:15-16), passes the resulting aggregate into
`new_with_aggregate`, and then `.finalize()` immediately overwrites it by
re-scanning the same files and findings (analysis_report.rs:135). The aggregate
argument is thus discarded on every production call, and the second scan's
evidence metrics are likewise thrown away. `AnalysisReport::new` (cfg(test),
48-67) and `aggregate` (163-168) are a parallel constructor family for the same
value.

**Recommendation:** Delete `FinalizedReportAggregate` and store
`summary: AnalysisReportSummary` on `AnalysisReport` directly. Drop the
`aggregate` parameter from the constructor; make `finalize()` (the canonical
completion used by `combine`, `with_project_diagnostics`, and `into_partial`)
the single owner of summary computation, and keep one `from_parts`-style scan
whose evidence metrics are returned only for seeding the operation counts in
`assemble_project_report`. Guardrails: preserve deterministic sorted ordering,
`ReportCompletion::join`, saturating `operations` merge, serde output (the
aggregate is already `serde(skip)`), and the invariant that the summary always
reflects the current files/diagnostics after `finalize`.

**Fix Applied:** None so far.

#### [ ] READ-002 — `AnalysisOperationCountsBuilder` duplicates the whole DTO as a write-side proxy

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/project/types/report/operations.rs:80-141`, call sites in `analysis/project/model.rs:438-451`, `lint/report/summary.rs:18-27`, `lint/report/mod.rs:157`

`AnalysisOperationCountsBuilder` wraps one `counts: AnalysisOperationCounts`
field and, aside from `finish()`, contains only 13 `record_*` methods that each
assign one field of the wrapped DTO (operations.rs:86-136) — a verbatim mirror
of the DTO's 13 fields and 13 getters. It is crate-private and always consumed
immediately inside one call chain: `ProjectSemanticModel::operation_counts()`
fills five counts, `assemble_project_report` fills eight more, then `finish()`
returns the DTO. Since `AnalysisOperationCounts` already has private fields,
public getters, `Default`, and saturating `AddAssign` (the two callers never
need a partially-built value across an owning boundary), the builder adds a
whole parallel write surface without enforcing any invariant.

**Recommendation:** Delete `AnalysisOperationCountsBuilder`; move the
`record_*` family onto `AnalysisOperationCounts` as `pub(crate)`
`record_*(&mut self)` methods, have `operation_counts()` return a value-seeded
`AnalysisOperationCounts` (metrics remain `Copy`), and let `summary.rs` mutate a
local before dropping it into the report. Guardrails: keep `Default` (the
harness profiles use it), keep public getters immutable, and preserve the
saturating `AddAssign` used by `AnalysisReport::merge` plus the
`max_live_alternatives`-via-`max` rule.

**Fix Applied:** None so far.

### Diagnostic construction

#### [ ] READ-003 — `AnalysisDiagnostic::set_location` is a whole-state mutator that grafts fabricated locations onto a public DTO

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/types/report/diagnostic.rs:34-36`, `glass-lint-core/src/lint/report/diagnostics.rs:9-25`

`AnalysisDiagnostic` is otherwise an immutable public report value, but it
exposes `pub(crate) fn set_location` (diagnostic.rs:34-36) that rebinds its
`Option<SourceLocation>` after construction. Its only caller is
`attach_project_diagnostics` (diagnostics.rs:14-23), which patches every
file-scoped status diagnostic with a fabricated one-byte range at `(1,1)` built
through four `Position::new`/`SourceRange::new` `expect`s. The two-phase
construction lets the diagnostic exist with no location and then be retargeted,
so any future caller can silently move a diagnostic's span, and the
report-type family's otherwise strict "validate at construction" posture is not
maintained here.

**Recommendation:** Delete `set_location`; build file-scoped
`Diagnostic::Project` diagnostics with their `SourceLocation` at the assembly
site (either extend `AnalysisStatus::diagnostics` to carry a path, or construct
the `AnalysisDiagnostic` with the resolved location in
`attach_project_diagnostics`), keeping `AnalysisDiagnostic` immutable.
Guardrails: do not drop or re-bucket status diagnostics, and keep the
deterministic per-file grouping; whether the sentinel `(1,1)` range should stay
or be replaced by `None` is captured in Open Questions.

**Fix Applied:** None so far.

### Summary classification

#### [ ] READ-004 — Parse/Project diagnostic classification is re-inlined in `FileReport` and the summary scan

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/types/report/file_report.rs:45-61`, `glass-lint-core/src/project/types/report/analysis_report.rs:208-234`

The same pair of `matches!` predicates — `Diagnostic::Parse { .. }` and
`Diagnostic::Project(_)` — is written twice: as `has_parse_diagnostics` /
`parse_diagnostic_count` on `FileReport` (file_report.rs:45-61) and again
inside `FinalizedReportAggregate::from_parts` when counting
`parse_diagnostics`, `file_diagnostics`, and `report_diagnostics`
(analysis_report.rs:213, 217-219, 232). The scan could consume
`FileReport::parse_diagnostic_count()` and a parallel `project_diagnostic_count`
for its per-file pass instead of re-running the same variant checks; today the
two implementations must be kept synchronized whenever the `Diagnostic` variant
set grows.

**Recommendation:** Classify each `Diagnostic` variant once on the owning type
(a `Diagnostic::is_parse()`/`is_project()` pair or per-file count methods on
`FileReport`) and have `from_parts` reuse those, with the report-level pass
still filtering the top-level `diagnostics` slice. Guardrails: preserve the
exact split (per-file parse counts, per-file project counts, report-level
project counts) and the existing summary field semantics, and keep the counts
deterministic and cheap (one scan, no nested re-traversals).

**Fix Applied:** None so far.

## Systemic Themes

- **Denormalized derived numbers are recomputed by consumers.** `AnalysisReportSummary`
  is skipped in serde output and must be kept consistent with the
  `files()`/`diagnostics()` slices through the single `finalize` path
  (READ-001). Meanwhile consumers re-derive aggregates ad hoc:
  `lint/report/mod.rs:253` computes a "diagnostics" total as
  `report.diagnostics().len() + report_summary.parse_diagnostics()`, which omits
  the already-available `file_diagnostics` component; `cli/output.rs:342-353`
  re-checks the summary counters for the clean/summary line. A single
  `AnalysisReportSummary`-owned total would remove the arithmetic split.
- **Two public `files` counters with different definitions.** `summary().files()`
  counts `FileReport` entries, including parse-failed files, while
  `operations().files()` counts linked modules (`model.rs:440`). Both are public
  on the same report under the same name; the divergence is real (parse failures
  produce a file report but no linked module) and could confuse consumers.
- **Positive: the validation boundary is component-based and uniform.** Ranges
  are validated at `SourceRange::new`/`Position::new` and against source text in
  `SourceLineIndex::try_range` before reaching `SourceLocation`; paths via
  `ProjectRelativePath::new`; codes via `DiagnosticCode::new`. `DiagnosticKind`
  keeps a single hand-maintained `as_str` table with a generated `ALL` under
  `cfg(test)` and a table test that cross-checks `as_str`, so drift between
  variant and serialized spelling is caught.
- **Growth risk: `DiagnosticKind.as_str` duplicates the variant list
  textually.** The macro contains the variant list and the match re-lists every
  name; the table test mitigates but does not eliminate the sync risk when kinds
  are added or renamed.

## Open Questions

1. Should `FinalizedReportAggregate::from_parts`'s evidence metrics
   (`evidence_steps`, `rendered_traces`) be packaged with the summary into one
   small record so the single-scan contract is explicit, or should they move
   into the `AnalysisOperationCounts` pipeline entirely (their only consumer)?
   This decides how aggressively READ-001's single scan is structured.
2. Is the fabricated one-byte `(1,1)` range for file-scoped status diagnostics
   (README-003) an intentional rendering decision, or should those diagnostics
   carry `None` so consumers can display "no range"? Either way the
   `set_location` mutator is removable.
3. Is the `summary().files()` vs `operations().files()` divergence intentional
   and stable, and should the summary field be renamed (e.g. `reported_files`)
   to avoid the collision with `operations().files()`?
4. `Diagnostic::Project` vs `Diagnostic::Parse` today approximate a shared
   `(code, message, Option<path>, Option<range>)` core through the `inner()`
   tuple (diagnostic.rs:59-81). A flattened `DiagnosticCore` would give the
   summary scan and the accessor family one classification, but it crosses the
   chunk boundary into `ParseDiagnostic` (`parse.rs`) and `AnalysisStatus`
   (`analysis/semantic/status.rs`), so it is recorded here rather than
   recommended.

## Coverage

Files read in full and cited in findings:

- `glass-lint-core/src/project/types/mod.rs`
- `glass-lint-core/src/project/types/report/mod.rs`
- `glass-lint-core/src/project/types/report/analysis_report.rs` (+ `analysis_report/tests.rs`)
- `glass-lint-core/src/project/types/report/code.rs` (+ `code/tests.rs`)
- `glass-lint-core/src/project/types/report/diagnostic.rs`
- `glass-lint-core/src/project/types/report/evidence.rs` (+ `evidence/tests.rs`)
- `glass-lint-core/src/project/types/report/file_report.rs`
- `glass-lint-core/src/project/types/report/finding.rs`
- `glass-lint-core/src/project/types/report/location.rs`
- `glass-lint-core/src/project/types/report/operations.rs`
- `glass-lint-core/src/project/report/mod.rs`

Files read for callers/context (not in chunk scope):

- `glass-lint-core/src/lint/report/mod.rs`, `summary.rs`, `evidence.rs`,
  `diagnostics.rs`, `files.rs`
- `glass-lint-core/src/analysis/project/model.rs`
- `glass-lint-core/src/analysis/project/linker/export.rs`
- `glass-lint-core/src/analysis/project/model.rs`
- `glass-lint-core/src/analysis/semantic/status.rs`
- `glass-lint-core/src/diagnostic.rs`, `glass-lint-core/src/parse.rs`
- `glass-lint-datastructures/src/diagnostic.rs`
- `glass-lint-cli/src/output.rs`, `config.rs`
- `glass-lint-output/src/report/render.rs`
- `glass-lint-harness/src/types/protocol.rs`, `runner.rs`, `runner/tests.rs`
- `glass-lint-project/src/loader.rs`