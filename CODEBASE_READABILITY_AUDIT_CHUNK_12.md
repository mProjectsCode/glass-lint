# Codebase Readability Audit

## Summary

Chunk 12 owns the public project boundary: normalized paths and source inputs,
resolution requests and outcomes, staged local-analysis transitions, bounded
execution callbacks, artifact handoff, report combination, and report value
types. The phase-state design is strong: consuming transitions prevent
linking before local analysis and resolution validation, source text is shared
by handle, resolver answers are checked against authored request identities,
and deterministic ordering is explicit. Two boundary contracts still allow
state that is surprising or structurally ambiguous: batch source admission is
not atomic on input error, and report combination does not preserve the
one-file-per-path identity established by normal project assembly.

## Findings

### Project input admission

#### [ ] READ-042 — Make multi-source admission atomic before local analysis

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/session/mod.rs:234-245, 324-333`; insertion boundary `glass-lint-core/src/project/tables.rs:13-25`

`ProjectCollection::analyze_sources` calls `admit_sources`, which applies
`SourceTable::insert` one source at a time with `try_for_each`. If a later
source is a duplicate or has an invalid path, the method returns an input
error after earlier sources have already been admitted into the live
collection. The caller receives no indication of which prefix was retained,
and a subsequent `finish_local` or retry observes a partially changed
session. The same incremental behavior is shared by the test-controlled
batch entry points.

This splits ownership of the batch admission contract between the public
session method and the mutable table. `SourceTable` correctly rejects each
individual duplicate, but the higher-level operation is described as one
admission-and-analysis transition and does not define partial success. It
also makes error recovery depend on the position of the failing item rather
than on an explicit transaction or a returned remainder.

**Recommendation:** Stage the entire incoming iterator in a temporary
validated table, checking both duplicates against the existing collection and
duplicates within the batch, then merge the staged table only after all inputs
pass. Choose atomic admission rather than adding an admitted-prefix/remainder
result: the current session transition has no useful partial-success contract.
Preserve normalized path ordering and existing single-source duplicate
behavior, and document the batch atomicity.

**Fix Applied:** None so far.

### Combined report identity

#### [ ] READ-043 — Reject or explicitly merge duplicate file paths when combining reports

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/report/mod.rs:5-70`; append/finalize implementation `glass-lint-core/src/project/types/report/analysis_report.rs:94-110`; file identity `glass-lint-core/src/project/types/report/file_report.rs:5-49`

Normal project assembly stores file reports in a `BTreeMap` keyed by
`ProjectRelativePath` and emits one `FileReport` per admitted source. The
public `AnalysisReport::combine` API validates only schema and tool version,
then `AnalysisReport::merge` appends both reports’ `Vec<FileReport>` values
and `finalize` sorts them by path. Nothing rejects two reports containing the
same path, and `ReportCombineError` has no duplicate-file case.

Combining two independently produced reports for `same.js` therefore yields
two file entries with the same identity. Findings, parse diagnostics,
summary counts, and serialized consumers can then disagree about whether the
path represents one file with combined evidence or two independent files.
Sorting makes the output deterministic but does not restore the uniqueness
invariant owned by the project session.

**Recommendation:** Reject duplicate paths universally at the report-combine
boundary and add a typed duplicate-path error. Validate schema, tool identity,
and the complete path set before moving any report contents, preserving the
transactional no-partial-result behavior. Do not add an analysis identity or
same-file merge policy without a current use case; keep normal project
assembly's one-file-per-normalized-path contract and deterministic ordering.

**Fix Applied:** None so far.

## Systemic Themes

- The consuming session types provide strong phase ownership, but collection
  methods that admit multiple inputs still need an explicit transaction
  boundary when they can fail partway through.
- Project paths are semantic identity values throughout source tables,
  module IDs, locations, and file reports. Public report combination should
  preserve that identity invariant rather than treating reports as arbitrary
  concatenable vectors.
- Deterministic sorting is used consistently, but ordering cannot substitute
  for uniqueness or atomicity. Those invariants should be enforced at the
  owning collection boundaries.

## Decisions

- A failed multi-source admission leaves the collection unchanged. An
  admitted-prefix/remainder result would create a second session state model
  without a current caller that can use it safely.
- Reject same-path report combination universally. Reports currently carry no
  analysis identity or merge semantics, so duplicate paths cannot be combined
  losslessly or deterministically at the finding/diagnostic level.
- Validate the complete path set before moving report contents, after the
  existing schema and tool-version checks. This keeps duplicate-path errors
  transactional and makes the one-file-per-path invariant explicit.

## Coverage

Reviewed only Chunk 12, “Project sessions, inputs, and reports,” from
`CODEBASE_STRUCTURE_CORE.md`, including normalized project paths and source
text, package/builtin/outside targets, source and resolution request values,
resolver outcomes and error taxonomy, source/resolution tables, authored
request indexing, analysis artifacts, cache-aware local transitions, bounded
worker execution and observer hooks, consuming collection/local/resolved
project states, report combination, report diagnostics/findings/evidence,
operation counts, completion values, and public report accessors. Existing
Chunk 1 through Chunk 11 audit history was used to continue IDs at READ-042.
No source, test, configuration, dependency, or other documentation files
were changed; this chunk audit file is the only new artifact for Chunk 12.
