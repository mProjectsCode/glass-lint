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
Each recommendation is scoped to its natural owner: the report type that owns
the state, the scan that owns the computation, or the type that owns the
variants. All four open questions raised by these findings were resolved
against the code; see "Open Questions — Resolved".

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
evidence metrics are likewise thrown away (finalize's `Self::aggregate`
recomputes `evidence_steps`/`rendered_traces` only to drop them,
analysis_report.rs:163-168). `AnalysisReport::new` (cfg(test), 48-67) and
`aggregate` (163-168) are a parallel constructor family for the same value.

**Recommendation:** Delete `FinalizedReportAggregate` and store
`summary: AnalysisReportSummary` on `AnalysisReport` directly. Drop the
`aggregate` parameter from the constructor; make `finalize()` (the canonical
completion used by `combine`, `with_project_diagnostics`, and `into_partial`)
the single owner of summary computation, and keep one scan that returns only the
evidence metrics for seeding the operation counts in `assemble_project_report`
(their sole consumer, summary.rs:19,27). Each derived value then lives in one
computation: the metrics scan in `assemble_project_report` and the summary scan
in `finalize` replace today's two full scans that each compute half of the other
scan's output and discard it. Guardrails: preserve deterministic sorted
ordering, `ReportCompletion::join`, saturating `operations` merge, serde output
(the aggregate is already `serde(skip)`), and the invariant that the summary
always reflects the current files/diagnostics after `finalize`.

**Fix Applied:** None so far.

#### [ ] READ-002 — `AnalysisOperationCountsBuilder` duplicates the whole DTO as a write-side proxy

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/project/types/report/operations.rs:79-141`, call sites in `analysis/project/model.rs:438-451`, `lint/report/summary.rs:18-27`, `lint/report/mod.rs:157`

`AnalysisOperationCountsBuilder` wraps one `counts: AnalysisOperationCounts`
field and, aside from `finish()`, contains only 13 `record_*` methods that each
assign one field of the wrapped DTO (operations.rs:86-136) — a verbatim mirror
of the DTO's 13 fields and 13 getters. It is crate-private and consumed
immediately at every call site: `ProjectSemanticModel::operation_counts()` fills
five counts (model.rs:440-450), `assemble_project_report` fills eight more
before `finish()` (summary.rs:18-27), and the link stage drains the five-count
result straight into `finish()` for logging (mod.rs:157). Since
`AnalysisOperationCounts` already has private fields, public getters, `Default`,
and saturating `AddAssign`, and no call site needs a partially-built value
across an owning boundary, the builder adds a whole parallel write surface
without enforcing any invariant.

**Recommendation:** Delete `AnalysisOperationCountsBuilder`; move the
`record_*` family onto `AnalysisOperationCounts` as `pub(crate)`
`record_*(&mut self)` methods, have `operation_counts()` return a value-seeded
`AnalysisOperationCounts` (metrics remain `Copy`), let `summary.rs` mutate a
local before dropping it into the report, and update the link-logging caller to
read `project.operation_counts()` directly (it no longer needs `finish()`).
Guardrails: keep `Default` (the harness profiles build counts from it,
e.g. `profile/runner/projects.rs:24`, `profile/metrics.rs:75`), keep public
getters immutable, and preserve the saturating `AddAssign` used by
`AnalysisReport::merge` plus the `max_live_alternatives`-via-`max` rule
(operations.rs:143-164).

**Fix Applied:** None so far.

### Diagnostic construction

#### [x] READ-003 — `AnalysisDiagnostic::set_location` is a whole-state mutator that grafts fabricated locations onto a public DTO

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/types/report/diagnostic.rs:34-36`, `glass-lint-core/src/lint/report/diagnostics.rs:9-25`

`AnalysisDiagnostic` is otherwise an immutable public report value, but it
exposes `pub(crate) fn set_location` (diagnostic.rs:34-36) that rebinds its
`Option<SourceLocation>` after construction. Its only caller is
`attach_project_diagnostics` (diagnostics.rs:16-23), which patches every
file-scoped status diagnostic with a fabricated one-byte range at `(1,1)` built
through four `Position::new`/`SourceRange::new` `expect`s. The two-phase
construction lets the diagnostic exist with no location and then be retargeted,
so any future caller can silently move a diagnostic's span, and the
report-type family's otherwise strict "validate at construction" posture is not
maintained here.

**Recommendation:** Delete `set_location` and construct the file-scoped
diagnostic with its `SourceLocation` at the point that owns the scope-to-path
mapping: `AnalysisStatus::diagnostics()` already returns each file-scoped
diagnostic paired with its path (status.rs:176,182) yet builds it with `None`
(status.rs:283), so have it build the location there and let
`attach_project_diagnostics` push the already-located `Diagnostic::Project`
values without mutation. `AnalysisDiagnostic` stays immutable. Guardrails: do
not drop or re-bucket status diagnostics, keep the deterministic per-file
grouping, and keep the `(1,1)` sentinel range — its path is the real payload,
and `SourceLocation` couples path and range (location.rs:7-15), so replacing it
with `None` would strip the file association and change the serialized shape
(see Open Questions — Resolved, #2).

**Fix Applied:** Removed `AnalysisDiagnostic::set_location` and construct the
sentinel file location while `AnalysisStatus` still owns the file path. Report
assembly now forwards already-located diagnostics without mutating them.

### Summary classification

#### [x] READ-004 — Parse/Project diagnostic classification is re-inlined in `FileReport` and the summary scan

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/types/report/file_report.rs:45-49,55-61`, `glass-lint-core/src/project/types/report/analysis_report.rs:208-234`

The same pair of `matches!` predicates — `Diagnostic::Parse { .. }` and
`Diagnostic::Project(_)` — is written twice: as `has_parse_diagnostics` /
`parse_diagnostic_count` on `FileReport` (file_report.rs:45-49, 56-60) and again
in the summary scan (currently `FinalizedReportAggregate::from_parts`, the
target of READ-001) when counting `parse_diagnostics`, `file_diagnostics`, and
`report_diagnostics` (analysis_report.rs:213, 215-219, 230-233). The scan could
consume `FileReport::parse_diagnostic_count()` and a parallel
`project_diagnostic_count` for its per-file pass instead of re-running the same
variant checks; today the two implementations must be kept synchronized whenever
the `Diagnostic` variant set grows.

**Recommendation:** Classify each `Diagnostic` variant once on the type that
owns the variants: add `Diagnostic::is_parse()`/`is_project()` in diagnostic.rs
and have both the `FileReport` predicates/counters and the summary scan reuse
them, with the report-level pass still filtering the top-level `diagnostics`
slice. Guardrails: preserve the exact split (per-file parse counts, per-file
project counts, report-level project counts) and the existing summary field
semantics, and keep the counts deterministic and cheap (one scan, no nested
re-traversals).

**Fix Applied:** Added `Diagnostic::is_parse()` and `Diagnostic::is_project()`
and reused them from `FileReport` and report summary aggregation, keeping the
existing per-file and report-level counter split unchanged.

## Systemic Themes

- **Denormalized derived numbers are recomputed by consumers.** `AnalysisReportSummary`
  is skipped in serde output (analysis_report.rs:43) and must be kept consistent
  with the `files()`/`diagnostics()` slices through the single `finalize` path
  (READ-001). Meanwhile consumers re-derive aggregates ad hoc:
  `lint/report/mod.rs:253` computes a "diagnostics" total as
  `report.diagnostics().len() + report_summary.parse_diagnostics()`, which omits
  the already-available `file_diagnostics` component; `cli/output.rs:342-353`
  re-checks the summary counters for the clean/summary line. A single
  `AnalysisReportSummary`-owned total would remove the arithmetic split.
- **Two public `files` counters with different definitions.** `summary().files()`
  is `files.len()` over the `FileReport` vector (analysis_report.rs:202),
  counting every source file including parse-failed ones (a parse failure still
  produces a `FileReport` with a `Diagnostic::Parse`, `files.rs:26-33`), while
  `operations().files()` counts linked modules (`model.rs:440`). Both are public
  on the same report under the same name; the divergence is real and structural
  (parse failures produce a file report but no linked module) and could confuse
  consumers. It is resolved as intentional in Open Questions — Resolved, #3.
- **Positive: the validation boundary is component-based and uniform.** Ranges
  are validated at `SourceRange::new`/`Position::new` and against source text in
  `SourceLineIndex::try_range` (diagnostic.rs:231) before reaching
  `SourceLocation`; paths via `ProjectRelativePath::new`; codes via
  `DiagnosticCode::new`. `DiagnosticKind`
  keeps a single hand-maintained `as_str` table with a generated `ALL` under
  `cfg(test)` and a table test that cross-checks `as_str`, so drift between
  variant and serialized spelling is caught.
- **Growth risk: `DiagnosticKind.as_str` duplicates the variant list
  textually.** The macro contains the variant list and the match re-lists every
  name; the table test mitigates but does not eliminate the sync risk when kinds
  are added or renamed.

## Open Questions — Resolved

1. **The evidence metrics belong in the `AnalysisOperationCounts` pipeline, not
   in a record packaged with the summary.** `aggregate_and_evidence` has exactly
   one caller (`summary.rs:15-16`), and its two metric outputs feed only
   `record_evidence` (`summary.rs:19`) and `record_rendered_traces`
   (`summary.rs:27`) — the two `AnalysisOperationCountsBuilder` methods with no
   other producer. A small record bundling them with the summary would create a
   new transient type with a single consumer for no gain. READ-001's single-scan
   contract is therefore: `assemble_project_report` keeps a metrics-only scan
   whose two values are consumed immediately by `record_evidence` /
   `record_rendered_traces`, and `finalize` remains the sole owner of summary
   computation. Each derived value is then computed exactly once across the two
   walks (the metrics walk and the summary walk), which is the same walk count
   as today but without the doubled summary/metrics computation.
2. **Keep the fabricated `(1,1)` range; it is not a rendering decision.** The
   scope is the real payload: status diagnostics are file-scoped by
   `StatusScope::File(path)` (status.rs:89), `AnalysisStatus::diagnostics()`
   builds every status diagnostic with `location: None` (status.rs:283) and
   returns the file-scoped ones already paired with their path (status.rs:182),
   and `SourceLocation` couples `path` with a mandatory `range` (location.rs:7-15)
   — so the sentinel exists solely to carry the path. No consumer reads the
   range itself: the pretty renderer reads only `Diagnostic::Parse` ranges
   (render.rs:400-466), the CLI iterates only `report.diagnostics()`
   (output.rs:294), which excludes file-scoped diagnostics, and the harness
   reads only messages (adapters.rs:180-187). Replacing it with `None` would
   strip the file association (path and range travel together) and change the
   serialized shape. READ-003's fix removes the `set_location` mutator while
   keeping the sentinel.
3. **The `summary().files()` vs `operations().files()` divergence is intentional
   and stable; do not rename.** The two counters measure different things:
   `summary().files()` is report file coverage (one `FileReport` per source,
   including parse failures, analysis_report.rs:202, files.rs:26-33), while
   `operations().files()` is a work metric over linked modules (model.rs:440);
   they are consumed in different contexts — `operations().files()` in the
   link/match tracing (mod.rs:161,250) versus `summary().files()` in the
   rendered summary line (output.rs:345) and the combine test
   (`project/report/tests.rs:205`). Each is derived once from its own immutable
   input and never crosses contexts, so a rename (e.g. `reported_files`) would
   be a public-API churn (output.rs, harness metrics) for cosmetic gain. The
   minimal consistent fix is READ-001: derive the summary count through the
   single `finalize` path.
4. **Flattening `Diagnostic::Project`/`Diagnostic::Parse` into a shared
   `DiagnosticCore` is correctly not recommended.** `Diagnostic::inner()`
   (diagnostic.rs:59-81) already centralizes the shared
   `(code, message, Option<path>, Option<range>)` projection, and its accessor
   consumers (diagnostic.rs:84-101) plus the summary scan are the only readers.
   A flattened core would merge `AnalysisDiagnostic` (diagnostic.rs:7-11) with
   `ParseDiagnostic` (parse.rs:31-41), which additionally carries a serde-skipped
   `failure: ParseFailureKind` (parse.rs:39-40) that status reporting depends on
   (`status.rs:173-174` skips `ParseFailure` entries precisely because the
   structured parser diagnostic owns that payload), and would change the serde
   shape of the `Diagnostic::Parse` variant. That crosses the chunk boundary
   into `parse.rs` and `status.rs`; the in-scope step is READ-004's variant
   classification on `Diagnostic`.

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
