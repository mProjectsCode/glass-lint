# Codebase Readability Audit — Chunk 11

This audit covers Chunk 11 of `CODEBASE_STRUCTURE_CORE.md`: lint execution
and reporting. It is an architectural review only; no source changes were
made.

## Summary

The linting layer preserves the important external guarantees: catalog
selection is deterministic, batch work is bounded and input-ordered, report
files are assembled in path order, and evidence remains tied to classified
occurrences. The main readability risks are at lifecycle boundaries. The
batch iterator owns both scheduling and protocol recovery, report assembly
exposes and coordinates internal phase state, evidence grouping reconstructs
relationships after discarding their references, occurrence references are
resolved repeatedly, and selector parsing/selection validation maintain
parallel policy paths.

## Findings

### Batch execution and report lifecycle

#### [ ] READ-001 — `BatchResults` combines scheduling, channel protocol, and iterator semantics

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/lint/batch.rs:102-313`; construction at `glass-lint-core/src/lint/linter.rs:190-212`
- **Representative callers:** `BatchResults::next` refills the worker window, closes the sender on input exhaustion, drains completions, synthesizes missing worker failures, and decides iterator termination; `PendingBatch` owns a second portion of the same lifecycle state

The public iterator contains the input iterator, Rayon pool, sender and
receiver, cancellation flag, pending reorder table, exhaustion flags, and
terminal state. Its `next` method coordinates submission, channel closure,
ordered readiness, worker-panic recovery, and end-of-stream behavior, while
`refill` receives cloned `Linter` and sender arguments even though the parent
iterator owns the associated protocol. The bounded-window invariant is
correct but distributed across `BatchResults`, `PendingBatch`, and the worker
closure, making cancellation or completion changes difficult to reason about.

**Recommendation:** Give a private batch driver one explicit transition for
submit, receive, recover, and yield, leaving `BatchResults::next` as a narrow
adapter over that transition. Keep `PendingBatch` as the owner of index
ordering and in-flight accounting, but move sender closure, cancellation,
and worker-result normalization into the driver rather than passing cloned
protocol pieces through helper arguments. Preserve the `max_in_flight`
bound, input ordering, `size_hint`, worker-panic conversion, and the contract
that dropping the iterator cancels queued work without claiming to interrupt
running jobs.

**Fix Applied:** None so far.

#### [ ] READ-002 — `ReportAssembly::finish` coordinates every report phase through one mutable orchestration procedure

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/lint/report.rs:122-186`; phase helpers at `glass-lint-core/src/lint/report/diagnostics.rs:13-83`, `evidence.rs:82-275`, and `summary.rs:10-64`
- **Representative callers:** `ResolvedProject::finish_with_timings` constructs one `ReportAssembly`; `finish` initializes files, links the project, records parse status, classifies, installs traces, populates evidence, attaches diagnostics, assembles summaries, and logs metrics

`ReportAssembly::finish` crosses several distinct ownership levels: source
diagnostic initialization, semantic linking, completeness-state mutation,
matcher projection, evidence rendering, diagnostic attachment, operation
counting, and final report construction. The helpers separate some syntax,
but the coordinator still owns the ordering and mutable handoff among all
phases, so a new report stage must be inserted into a function that also
controls timing, status, trace ownership, and telemetry.

**Recommendation:** Model the internal report pipeline as named phase
transitions—link result, matching result, rendered-file result, and finalized
report—so each transition owns the state it creates and the next phase gets a
typed input. Keep timing and logging at the phase boundary that owns the
work, and let the finalizer own operation-count completion and
`ReportCompletion`. Preserve parse-diagnostic placement, completeness
monotonicity, trace reconstruction, deterministic file/evidence order, and
the distinction between semantic matching and presentation failures.

**Fix Applied:** None so far.

#### [x] READ-003 — The internal report assembler is exported beside a raw timing DTO

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/lint/report.rs:20-24,107-128`; re-export at `glass-lint-core/src/lint/mod.rs:14-18`; sole caller at `glass-lint-core/src/project/session/mod.rs:455-466`
- **Representative callers:** only `ResolvedProject::finish_with_timings` constructs `ReportAssembly`, while the public `ProjectAnalysis` exposes `report`, `linking`, and `matching` as mutable-shape public fields

`ReportAssembly` is re-exported as a public core API even though it is an
internal bridge over `SourceTable`, `ResolvedLinkInput`, parse-diagnostic
maps, catalog indexes, and analysis limits, and the staged project session is
its only production caller. Its companion `ProjectAnalysis` exposes the
assembled report and timing fields directly, without an ownership or
stability boundary. This makes a private phase coordinator look like an
alternative public construction path and couples consumers to storage-shaped
timing data.

**Recommendation:** Make `ReportAssembly` crate-private and remove its public
re-export; keep the staged `ResolvedProject` methods as the sole report
construction boundary. Treat phase timing as a supported workspace-facing
result because `glass-lint-project` consumes it, but expose it through a small
documented `ProjectAnalysis` result with private fields and accessors or one
named timing value rather than raw mutable fields. Preserve the ordinary
`finish` convenience, profiling consumers, schema stability, and the consuming
project lifecycle that prevents linking or matching before resolution
validation.

**Fix Applied:** `ReportAssembly` is now crate-private and is no longer part of
the public lint facade. `ProjectAnalysis` keeps its report and timings private,
exposing documented timing accessors and a consuming `into_report` boundary;
workspace callers were updated accordingly. Verified with `make fmt && make ci`.

### Evidence and finding construction

#### [ ] READ-004 — Finding-range grouping discards evidence associations and reconstructs them by positional scanning

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Complexity
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:165-212`; containment helper at `glass-lint-core/src/lint/ranges.rs:5-31`
- **Representative callers:** `findings_for_capability` creates `EvidenceRangeEntry` values, extracts only cloned ranges for `remove_contained_ranges`, then `finding_groups` scans the original entries with `entry_cursor` and `FindingGroup::add_entry`

The evidence path first groups occurrences by `SourceRange`, then separates
the ranges from their occurrence references to perform containment reduction,
and finally tries to recover group membership through sorted positional
scanning. The relationship between a retained range and its evidence refs is
therefore represented in two parallel structures, with correctness depending
on both sort orders and the `contains`/overlap conditions in separate owners.
An adjustment to range containment or overlapping-group policy can silently
leave the retained display range and its trace references out of sync.

**Recommendation:** Give a private finding-range builder ownership of both
the ordering and the occurrence references, reducing contained entries and
forming groups in one domain operation. Delete the range-only reconstruction
and make the builder return groups with their refs already attached. Preserve
equal-range coalescing, containment and overlap semantics, deterministic
source ordering, evidence truncation, and the rule that every rendered trace
comes from the original classification occurrence.

**Fix Applied:** None so far.

#### [ ] READ-005 — `FindingGroup` resolves the same occurrence references in three separate traversals

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Encapsulation
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:47-78,233-271`
- **Representative callers:** `findings_for_capability` resolves refs while building traces, then `FindingGroup::is_truncated` and `FindingGroup::certainty` each walk and resolve the same refs again

`FindingGroup` stores private two-index references into classification
evidence, but its truncation and certainty methods each repeat the bounds
checked lookup already performed by the trace-building loop. A group with
many occurrences consequently has three separate interpretations of the
same reference set, and future evidence metadata would need to be threaded
through each traversal. The repeated `filter_map` also makes it less obvious
that invalid internal references are intentionally ignored.

**Recommendation:** Resolve a group’s references once into a private
aggregation view that can supply traces, certainty, and truncation state, or
let a group-owned iterator yield validated occurrence pairs to all consumers.
Keep fail-closed handling for invalid refs, definite-over-possible certainty,
truncation propagation, deterministic `BTreeSet` trace ordering, and the
fallback occurrence trace when no usable trace survives.

**Fix Applied:** None so far.

### Rule selection API

#### [ ] READ-006 — Selector parsing couples wildcard grammar to `RuleId` through a sentinel string

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Conversion
- **Location:** `glass-lint-core/src/lint/selection.rs:61-179`
- **Representative callers:** `RuleOverride::new` and serde deserialization call `RuleSelector::parse`; parsing validates `selector.replace('*', "placeholder")` with `RuleId::parse`, then `matches` interprets a separate segment representation

The selector type has its own wildcard language, but syntax validation is
performed by rewriting wildcard characters to the literal sentinel
`placeholder` and asking the rule-ID parser to accept the result. Matching
then uses a separately built segment list plus an end-wildcard flag, while
the raw selector is retained for serialization. The selector contract is
therefore coupled to an unrelated identity parser and several parallel
representations; changing either rule-ID grammar or wildcard grammar can
change the other’s accepted inputs without a typed boundary.

**Recommendation:** Give a private `RulePattern` parser ownership of the
wildcard grammar and its validated literal segments, and use an explicit
conversion for literal segments to `RuleId` only where exact-rule errors need
it. Derive wildcard anchoring from that canonical pattern instead of using a
sentinel validation path. Preserve exact selectors, leading/trailing and
adjacent wildcard behavior, serialized raw spelling, namespaced rule-ID
validation, and deterministic matching.

**Fix Applied:** None so far.

#### [ ] READ-007 — `RuleSelection::resolve` mixes effective-state calculation with unmatched-override diagnostics

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API / Complexity
- **Location:** `glass-lint-core/src/lint/selection.rs:212-278`
- **Representative callers:** `Linter::new` needs the enabled `RuleIndex` vector, while `RuleSelection::validate` calls `resolve` and discards that vector; the same method applies baseline/override precedence, tracks matches, and chooses `UnknownRule` versus `InvalidSelector`

`resolve` performs three related but distinct jobs in one loop: computes the
effective state for every catalog record, tracks which overrides matched, and
turns unmatched selectors into user-facing errors in a second pass. The
public validation method runs the full allocation-producing resolution just
to discard the enabled indexes. Because exact selectors and wildcard
selectors take different error branches, changes to selection precedence or
selector validity must also preserve the diagnostic protocol embedded in the
resolution loop.

**Recommendation:** Give `RuleSelection` a private evaluation result that
separates per-rule state from override-match validation, and let `resolve`
project that result to deterministic `RuleIndex` values. Let `validate` use
the validation portion without allocating an enabled vector, or expose one
canonical validated-selection transition for both callers. Preserve
last-matching-override-wins behavior, baseline confidence filtering, exact
`UnknownRule` versus wildcard no-match errors, catalog order, and fail-closed
configuration.

**Fix Applied:** None so far.

## Systemic Themes

- **ENCAPSULATE:** Batch protocol state, report construction, and selector
  grammar should be owned by private domain transitions rather than public
  orchestrators or sentinel conversions.
- **SIMPLIFY:** Batch iteration, report assembly, and selection validation
  currently coordinate multiple lifecycle levels in one procedure.
- **DEDUPLICATE:** Evidence range relationships and occurrence metadata are
  repeatedly reconstructed or resolved at adjacent presentation stages.

## Decisions

- `finish_with_timings` is a supported workspace-facing profiling boundary,
  not a public construction path for report assembly. Keep a narrow public
  timing result for `glass-lint-project`, hide `ReportAssembly`, and avoid
  exposing raw mutable timing fields as the long-term contract.
- Preserve `InvalidSelector` for a syntactically valid wildcard that matches
  no catalog rule. Internally separate selector evaluation from diagnostic
  mapping, but map an unmatched wildcard to the existing error so the public
  error surface remains small and exact selectors continue to report
  `UnknownRule`.

## Coverage

Reviewed batch options, pending-window accounting, worker completion and
cancellation, `Linter` construction and source/batch entry points, catalog
callers without re-reporting the prior catalog identity/index findings,
selector parsing and wildcard matching, baseline/override resolution, report
assembly and timing/result boundaries, parse/project diagnostic routing,
evidence range normalization and grouping, trace reconstruction, finding
certainty/truncation, summaries, and deterministic ordering. Existing tests
were not changed or run because this is a read-only audit.

## Handoff

Chunk 11 is complete. The next unreviewed chunk is **Chunk 12 — Project
sessions, inputs, and reports** (`CODEBASE_STRUCTURE_CORE.md` lines 762-837),
covering project sessions, local execution, source/resolution tables, input
types, diagnostics, findings, evidence, and report values.
