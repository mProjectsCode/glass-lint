# Codebase Readability Audit

## Summary

Chunk 05 (`analysis::local` and `analysis::semantic`) has clear phase
boundaries: semantic analysis produces immutable matcher-independent artifacts,
the cache retains reusable state while reattaching path-local source context,
and bounded completion is recorded before derived phases are enabled. The main
opportunities are that the status model has become a cross-pipeline ownership
point, and one diagnostic transport wrapper is immediately unpacked by its
only production consumer.

## Findings

### [analysis/semantic/status.rs, analysis/project, lint/report]

#### [ ] READ-014 — Separate local completion status from project/report status

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture/API
- **Location:** `glass-lint-core/src/analysis/semantic/status.rs:10-115,128-183,186-284`; `glass-lint-core/src/analysis/semantic/mod.rs:255-306`; `glass-lint-core/src/analysis/project/projection.rs:407-441`; `glass-lint-core/src/lint/report/mod.rs:63-100,189-205`

`analysis::semantic::status` owns `AnalysisStatus`, but its `IncompleteReason`
and diagnostic mapping cover the entire pipeline: local facts and parser
spans, function effects, flow, linking, evidence-capacity mismatches, and
invalid rule selection. `AnalysisStatus` is stored in reusable
`SemanticArtifact` values and is also copied into `ProjectSemanticModel` and a
report session. Consequently, a local-artifact status type imports project
diagnostic types and must be edited whenever a downstream projection or lint
report adds a new reason. The local completion owner also disables derived
capabilities, while project projection and report code reuse the same mutable
set for unrelated phase failures; scope materialization and diagnostic
presentation are therefore coupled through one enum and one storage type.

**Recommendation:** Split the local artifact’s completion state from the
project/report diagnostic accumulator. Keep a local-only status/reason type
next to `SemanticArtifact` and `AnalysisCompletion`, and let the project/report
layer own linking, flow, evidence, and rule-selection reasons through an
explicit aggregation API. Delete downstream variants from the local status
enum and the direct semantic-to-project diagnostic dependency once callers
have migrated. Preserve fail-closed derived-phase capability disabling,
deduplication and deterministic `BTreeSet` ordering, local-to-file status
materialization, parse-diagnostic suppression, and the final file/project
diagnostic partition.

**Fix Applied:** None so far.

### [analysis/semantic/status.rs, lint/report]

#### [ ] READ-015 — Delete the immediately consumed `StatusDiagnostics` wrapper

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API/Conversion
- **Location:** `glass-lint-core/src/analysis/semantic/status.rs:112-125,159-183`; `glass-lint-core/src/lint/report/mod.rs:95-97`; `glass-lint-core/src/lint/report/diagnostics.rs:49-71`

`AnalysisStatus::diagnostics` constructs `StatusDiagnostics` solely to carry
two vectors, and the only production caller immediately invokes
`StatusDiagnostics::into_parts` to recover those vectors. The wrapper exposes
no validation, ordering policy, or behavior beyond the tuple it contains; its
public(crate) re-exports add another status-facing type without giving callers
a meaningful abstraction. This makes a simple diagnostic projection look like
a durable domain object and adds a transport layer between the report session
and the report builder.

**Recommendation:** Return the two owned diagnostic collections directly from
the status projection (or provide a report-owned method that consumes them),
then delete `StatusDiagnostics`, `into_parts`, and their re-exports. Keep the
`BTreeSet` traversal order, the deliberate omission of `ParseFailure` status
entries, the distinction between file and project diagnostics, and the
location attachment performed by `diagnostics.rs`; do not move presentation
policy into the status accumulator while removing the wrapper.

**Fix Applied:** None so far.

## Systemic Themes

- The local/cache boundary is otherwise well-factored: reusable semantic state
  is reference-counted while source paths remain attached per project use.
- Completion capability state and diagnostic reporting are related but have
  different owners. Keeping them in one status enum makes local artifact
  evolution depend on every downstream pipeline phase.
- Small domain wrappers are valuable when they enforce invariants; a wrapper
  with one producer, one consumer, and only an `into_parts` projection should
  be removed or moved to the owner that gives it behavior.

## Open Questions

- Should project linking and matcher projection report into one project status
  accumulator, or should each phase return a typed outcome that the report
  session merges? The key invariant is that cached local artifacts remain
  independent of a particular project/report run.
- Is the `AnalysisStatus` snapshot intentionally part of cache identity, or is
  it purely an analysis result? Any status split must preserve cache reuse and
  avoid carrying project-local diagnostics into reusable artifacts.

## Coverage

Reviewed Chunk 05: local artifact cache keys and fingerprints; synchronized
FIFO cache; source-context and semantic-artifact sharing; local/project module
wrappers; semantic parsing and span normalization; resolved-program freeze;
semantic budgets; completion capabilities; status scopes, incomplete reasons,
diagnostics, materialization, and representative project/report consumers.
Read the root/core architecture, testing/contributing guidance, the complete
readability-audit skill instructions, and existing audits 001–004. No source
or test files were changed.
