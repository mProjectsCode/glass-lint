# Codebase Readability Audit

## Summary

Chunk 11 has explicit phase transitions for linking, matching, rendering, and
final report aggregation, and its batch driver correctly bounds in-flight work
while yielding input order. The main opportunities are execution ownership
and report materialization. Batch workers currently route independent files
through the full project-session pipeline, report grouping retains duplicate
range storage, and internal completion protocol failures are guarded only by
debug assertions.

## Findings

### [lint/linter.rs and lint/batch.rs]

#### [ ] READ-028 — Separate independent batch execution from project sessions

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/linter.rs:218-270`; `glass-lint-core/src/lint/batch.rs:199-306`; representative session pipeline `glass-lint-core/src/project/session/mod.rs:462-485`

`Linter::lint_source` creates a `ProjectSession`, analyzes one source, and
finishes it through validation, link-input construction, linking, matching,
rendering, and aggregation. `BatchDriver::refill` invokes that method once per
worker task, so a batch advertised as “independent owned sources” repeats the
full project-session/report state machine for every input. This keeps behavior
consistent, but it also makes batch throughput pay for project containers,
linking/report phase objects, and per-source aggregation that the batch API
does not combine. It obscures which work is required for a single-file report
and which work exists only for multi-file project semantics.

**Recommendation:** Profile the single-source path, then give the linter one
private source-report execution owner that can be used by both `lint_source`
and batch workers, or add a batch-specific local-analysis/report path when
project linking is provably unnecessary. Keep the public result and error
semantics identical, and retain the project-session path for sources whose
requests or project context require it. Preserve shared catalog/environment
configuration, artifact-cache behavior, parse and execution diagnostics,
deterministic file ordering, completion status, and the batch driver’s bounded
in-flight/input-ordered protocol. Add equivalence tests comparing single and
batch results for ordinary files, parse failures, imports, and incomplete
analysis statuses before deleting any duplicated path.

**Fix Applied:** `FindingRangeBuilder` now retains sorted entry indexes rather
than cloning every source range into a second collection. Grouping still scans
the canonical BTree-ordered entries, preserving containment, overlap,
occurrence, trace, and deterministic ordering behavior. Verified with
`cargo test -p glass-lint-core lint --lib`.

### [lint/report/evidence.rs and lint/ranges.rs]

#### [ ] READ-029 — Avoid duplicate range materialization during report grouping

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:172-239`; `glass-lint-core/src/lint/ranges.rs:5-31`

`FindingRangeBuilder::new` first groups occurrences in a
`BTreeMap<SourceRange, Vec<_>>`, converts that map into `entries`, clones every
range into a second `retained_ranges` vector, and then sorts/reduces that
second vector with `remove_contained_ranges`. `into_groups` scans both
collections to reconstruct groups. The duplicate range storage and second
containment pass are repeated for every classified capability, even though the
map has already supplied deterministic range order. The two representations
also make it harder to see whether a future change keeps entry order aligned
with retained-range order.

**Recommendation:** Let one private report-range collection own sorted entries
and retained-range selection, using entry indexes or an in-place sweep instead
of cloning every `SourceRange`. Preserve the existing containment rule,
overlap grouping, occurrence association, trace fallback, truncation/certainty
reduction, and deterministic ordering. Add tests for equal ranges, nested
ranges, overlapping non-contained ranges, disjoint ranges, and evidence with
invalid/empty source spans before removing either pass.

**Fix Applied:** `PendingBatch::complete` now returns an explicit protocol
result. Unknown and duplicate completions close the sender and convert every
pending entry to the existing deterministic `WorkerPanic` execution error,
preventing ordered iteration from waiting indefinitely. Regression tests cover
unknown and duplicate indexes. Verified with
`cargo test -p glass-lint-core lint::batch --lib`.

### [lint/batch.rs]

#### [ ] READ-030 — Make batch completion protocol failures recoverable

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/lint/batch.rs:107-180,240-305`

`PendingBatch::complete` treats an unknown or duplicate completion as a
debug-only invariant violation and returns without changing state in release
builds. Because `in_flight` is decremented only by `take_ready`, a malformed
completion can leave the driver waiting for a result that will never arrive;
the same protocol state is not represented in `BatchStartError` or any iterator
result. The current worker closure generates valid indexes, but the channel is
the ownership boundary between worker tasks and the ordered driver, and its
failure behavior should not depend on debug assertions.

**Recommendation:** Give `complete` an explicit internal protocol result (or
convert an invalid completion into a synthesized `WorkerPanic`/batch execution
failure), close or cancel the driver, and make the iterator terminate with a
deterministic result rather than wait indefinitely. Keep the existing
input-order buffering, cancellation on `Drop`, panic conversion, bounded
in-flight count, and successful worker result behavior. Add tests for unknown
indexes, duplicate indexes, sender closure, worker panic, and cancellation;
ensure the destination pending state cannot be silently corrupted in release
builds.

**Fix Applied:** None so far.

## Systemic Themes

- The report state machine makes ownership transitions legible; the execution
  path should expose the same distinction between project-required work and
  independent-source work.
- Deterministic output is valuable, but it should be achieved with one owned
  ordering/containment representation where possible rather than parallel
  collections that must remain aligned.
- Worker/channel protocols are expected-error boundaries. Debug assertions are
  useful development checks, but release behavior must terminate safely and
  report a stable failure when an internal protocol invariant is broken.

## Review Resolutions

- Keep `lint_source` as the canonical single-source semantic path until an
  equivalence test proves a private local/report fast path for ordinary,
  malformed, imported, and incomplete sources. READ-028 is an ownership and
  measured-cost question, not permission to duplicate project semantics.
- Prefer entry indexes or an in-place retained-entry sweep for READ-029. Do not
  introduce an interval-tree abstraction unless overlap behavior grows beyond
  the current deterministic containment pass.
- Keep protocol failures internal and convert them to the existing deterministic
  `WorkerPanic` result unless callers need to distinguish protocol corruption
  from a worker panic. Do not expand the public error schema for this invariant
  alone.

## Coverage

Reviewed Chunk 11: linter construction and source execution; bounded batch
options, worker pools, cancellation, ordered pending results, and panic/error
conversion; rule catalog/selection integration; report phase transitions;
diagnostic attachment; evidence range grouping and trace reconstruction;
finding deduplication; containment normalization; and report summary/metrics
assembly. Read the root/core architecture, testing/contributing guidance, the
complete readability-audit skill instructions, and existing audits 001–010.
No source or test files were changed.
