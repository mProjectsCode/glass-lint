# Codebase Readability Audit — Chunk 12

Scope: `CODEBASE_STRUCTURE_CORE.md` lines 762–837, covering project session
lifecycle, local-analysis artifacts, project tables and input types, and report
types.

This is a read-only audit. No production source was changed.

## Summary

The project layer has clear phase boundaries, but several state transitions are
represented by parallel collections, boolean mode switches, or positional
metric arguments. Those shapes make lifecycle invariants harder to see at the
call site and allow public APIs to expose types that only make sense after the
internal link phase. The most valuable changes would be to encapsulate the
per-source artifact state and separate the local-analysis modes; the remaining
findings are smaller API and reporting improvements.

| ID | Category | Finding | Impact | Confidence |
|---|---|---|---|---|
| READ-001 | SIMPLIFY | `LocalAnalysisTransition::prepare` has two lifecycle contracts behind `skip_completed` | High | High |
| READ-002 | ENCAPSULATE | `AnalysisArtifacts` encodes one per-source state in two maps plus absence | High | High |
| READ-003 | ENCAPSULATE | Project public exports expose linker-only `LinkedModuleTarget` and `ModuleId` | High | High |
| READ-004 | ENCAPSULATE | `EvidenceTraces` exposes a boolean mode alongside a constrained trace vector | Medium | High |
| READ-005 | ENCAPSULATE | Operation path metrics cross the report boundary as six positional values | Medium | High |
| READ-006 | DEDUPLICATE | `AnalysisReport::summary` repeatedly traverses nested report data | Low | High |
| READ-007 | ENCAPSULATE | `SourceTable` documents insertion order while implementing normalized path order | Low | High |

## Findings

#### [x] READ-001 — Split the two `LocalAnalysisTransition::prepare` contracts

- **Category:** SIMPLIFY
- **Location:** `glass-lint-core/src/project/session/mod.rs:116-145, 173-181, 245-275`
- **Impact:** High — Complexity / Architecture
- **Confidence:** High
- **Evidence:** `prepare(candidate, skip_completed)` uses the boolean to choose
  whether an already completed path is skipped. The callback implementation
  always passes `true` for executor-driven jobs, while the synchronous
  `analyze_source_at_path_with_observer` path passes `false`, then manually
  performs `lower` and `complete`. The same method also owns cache-hit
  recording, cache-miss job construction, and observer events. A caller must
  understand both the boolean's meaning and which surrounding protocol it is
  expected to run.
- **Why this is a readability issue:** One private method represents two
  policies: batch scheduling may skip a completed source, whereas an explicit
  single-source request forces the preparation check and runs the job inline.
  The boolean does not communicate that policy at the call site, and the
  inline path duplicates the callback's prepare/lower/complete lifecycle.
- **Recommendation:** Treat the synchronous single-source path as an explicit
  force-reanalysis operation and give it a named method; give executor jobs a
  separate `prepare_pending` operation. Share only the lower/complete
  primitives where their contracts are identical. Keep cache hit/miss behavior,
  `ExecutionEvent` ordering, parse failure handling, and deterministic request
  collection unchanged. Tests should continue to cover explicit reanalysis,
  executor skips, cache hits, cache misses, and observer event sequences.
- **Guardrails:** Do not make the executor aware of `AnalysisArtifacts` or move
  the cache into the public collection API. The refactor should preserve the
  single-source force semantics and the batch scheduler's skip behavior.

**Fix Applied:** The transition now has named `prepare_pending` and
`prepare_requested` paths, with cache lookup shared by `prepare_cached`.
Explicit single-source analysis is contained in `analyze_requested`, while
executor callbacks use the pending-source contract. Verified with
`make fmt && make ci`.

#### [x] READ-002 — Encapsulate the per-source local-analysis state

- **Category:** ENCAPSULATE
- **Location:** `glass-lint-core/src/project/session/artifacts.rs:56-61,
  103-188`
- **Impact:** High — Architecture / Correctness
- **Confidence:** High
- **Evidence:** `AnalysisArtifacts` stores `analyzed` and
  `parse_diagnostics` as independent `BTreeMap`s. `needs_analysis` defines a
  third state through absence in both maps. `record_parse_failure` removes an
  analyzed entry before inserting a diagnostic, while `record_lowered`
  inserts an analyzed entry without clearing an existing parse diagnostic.
  `validate_complete` and `needs_analysis` then infer lifecycle state from
  those collections. `into_link_input` later destructures the maps and splits
  diagnostics back out for reporting.
- **Why this is a readability issue:** The invariant is “one path has pending,
  a successful local artifact, or a parse failure,” but the type does not own
  that invariant as a single value. In particular, a retry or alternate
  single-source path can leave a stale parse diagnostic alongside a successful
  artifact unless every caller knows the implicit replacement rule. The
  authored-request table adds a related phase index, making the state harder
  to reason about from one method.
- **Recommendation:** Introduce a private per-path outcome/state abstraction,
  such as `LocalAnalysisState::{Pending, Analyzed(LocalArtifact),
  ParseFailed(ParseDiagnostic)}`, or centralize the same invariant in one
  owner map with projection methods for link input and reporting. Make the
  transition methods responsible for replacing the prior outcome atomically.
  Preserve the separate authored-request membership index if it is needed for
  resolver validation, but do not expose storage details to callers.
- **Guardrails:** Keep parse diagnostics available for partial reports, keep
  successful retry semantics explicit, retain path-order completion checks, and
  preserve the consuming `AnalysisArtifacts -> ResolvedLinkInput` transition.
  Tests should cover success after failure, failure after success, incomplete
  paths, authored-request validation, and cache-produced artifacts.

**Fix Applied:** `AnalysisArtifacts` now stores one private
`LocalAnalysisOutcome` per path, making analyzed and parse-failed states
mutually exclusive while retaining authored-request indexing separately.
Consuming link-input construction projects those outcomes into linker modules
and diagnostics. Verified with `make fmt && make ci`.

#### [x] READ-003 — Narrow the public project API around post-link identities

- **Category:** ENCAPSULATE
- **Location:** `glass-lint-core/src/project/mod.rs:13-25`,
  `glass-lint-core/src/project/types/input.rs:390-429`,
  `glass-lint-core/src/analysis/project/model.rs:200-217`
- **Impact:** High — API / Architecture
- **Confidence:** High
- **Evidence:** The project root publicly re-exports both `LinkedModuleTarget`
  and opaque `ModuleId`. `ResolverOutcome` is the public resolver input, while
  the only production conversion to `LinkedModuleTarget` maps an internal
  `ProjectRelativePath` to a `ModuleId` during linking. `ModuleId::new` is
  crate-private and the type has no public accessor; external callers can
  neither construct a meaningful ID nor use the linked target as a stable
  input contract.
- **Why this is a readability issue:** The public namespace advertises an
  internal post-resolution representation beside the caller-facing resolver
  outcome. That makes the phase boundary less legible and invites consumers to
  depend on an identity type whose assignment is an internal path-ordering
  detail. The opaque type is useful inside the analysis model, but its public
  export does not currently provide a useful external operation.
- **Recommendation:** Make `LinkedModuleTarget` and `ModuleId` crate-private;
  the current workspace search found no downstream caller that needs these
  linker-only identities, and no public constructor/accessor makes them a
  useful external contract. Preserve the explicit `ResolverOutcome -> linked
  target` conversion and opaque IDs internally. If a later consumer needs
  linked results, add a deliberate read-only semantic view rather than
  re-exporting the storage identity.
- **Guardrails:** Audit all workspace callers before changing visibility;
  preserve serialized report behavior and internal analysis ownership. Do not
  replace the opaque ID with a path in the internal graph merely to simplify
  the public surface.

**Fix Applied:** `ModuleId` and `LinkedModuleTarget` are no longer part of the
public project facade or public type re-exports. Internal analysis and linker
callers use a crate-private project re-export, while `ResolverOutcome` remains
the caller-facing resolution contract. Verified with `make fmt && make ci`.

#### [ ] READ-004 — Make the valid `EvidenceTraces` states explicit

- **Category:** ENCAPSULATE
- **Location:** `glass-lint-core/src/project/types/report/evidence.rs:85-157`
- **Impact:** Medium — API / Correctness
- **Confidence:** High
- **Evidence:** `EvidenceTraces` stores `Vec<EvidenceTrace>` plus a `truncated`
  boolean. `with_truncation` rejects an empty non-truncated vector but accepts
  an empty truncated vector; `merge` manually combines vectors and ORs the
  flag; `fallback` manually constructs the non-truncated representation.
  The public constructor therefore accepts a pair whose validity depends on a
  relationship between fields, and several constructors encode that
  relationship independently.
- **Why this is a readability issue:** The meaningful states are “complete
  traces” and “truncated traces,” with an empty trace list only valid in the
  latter state. A bare boolean makes that invariant easy to violate or
  misread, especially when new construction paths are added. The manual
  struct literals in `merge` and `fallback` also bypass one central invariant
  gate.
- **Recommendation:** Use private state constructors or an internal enum
  (`Complete(Vec<EvidenceTrace>)` / `Truncated(Vec<EvidenceTrace>)`) and expose
  named operations such as `complete`, `truncated`, and `fallback`. If the
  serialized shape must remain unchanged, keep the enum private and preserve
  the current `traces` plus optional `truncated` representation at the serde
  boundary.
- **Guardrails:** Preserve the valid empty-truncated state, deterministic sort
  and deduplication in `merge`, fallback occurrence semantics, and existing
  serialization compatibility. Keep `EvidenceConstructionError` behavior for
  empty complete traces.

#### [ ] READ-005 — Pass path metrics as a semantic aggregate

- **Category:** ENCAPSULATE
- **Location:** `glass-lint-core/src/project/types/report/operations.rs:79-133`,
  `glass-lint-core/src/lint/report/summary.rs:39-50`,
  `glass-lint-core/src/analysis/project/projection.rs:367-385`
- **Impact:** Medium — API / Duplication
- **Confidence:** High
- **Evidence:** `ProjectionMetrics` owns six related dimensions:
  `max_live_alternatives`, `trace_heads`, `coalescing_comparisons`,
  `fixed_point_iterations`, `effect_projections`, and an operation count.
  Report assembly unpacks those values, adds a session-owned trace-node count
  and a file-derived rendered-trace count, and passes six positional values to
  `AnalysisOperationCountsBuilder::record_path_metrics`. The builder then
  reassembles them into the report DTO.
- **Why this is a readability issue:** The positional call has no type-level
  indication that the third argument is trace heads or that rendered traces
  come from a different owner. Adding or reordering a metric requires
  coordinated edits at the projection type, report assembly, builder method,
  and tests, with plausible same-typed values compiling in the wrong slot.
- **Recommendation:** Introduce a private named report-metrics aggregate or a
  conversion owned by `AnalysisOperationCountsBuilder`, with explicit fields
  for semantic projection metrics, session trace nodes, and rendered traces.
  Keep the public `AnalysisOperationCounts` DTO stable unless a public schema
  change is intentional.
- **Guardrails:** Preserve the distinction between semantic operations and
  user-facing rendered traces, current additive/max aggregation behavior, and
  deterministic counts. Do not fold wall-clock timing or budget status into
  this count object.

#### [x] READ-006 — Centralize report-summary aggregation

- **Category:** DEDUPLICATE
- **Location:** `glass-lint-core/src/project/types/report/analysis_report.rs:187-210`,
  related report assembly at `glass-lint-core/src/lint/report/summary.rs:17-50`
- **Impact:** Low — Duplication / Maintainability
- **Confidence:** High
- **Evidence:** `AnalysisReport::summary` independently traverses `files` for
  file count, findings, parse diagnostics, and file project diagnostics, then
  traverses report-level diagnostics for report diagnostics. Report assembly
  separately traverses file findings and every evidence trace to compute
  evidence and rendered-trace operation counts. These are related aggregate
  views over the same finalized report tree but have separate traversal and
  filtering logic.
- **Why this is a readability issue:** The split makes the ownership of report
  aggregates unclear and means a new diagnostic or evidence category may need
  multiple independently maintained filters. It also prevents a reader from
  finding one canonical description of the report's aggregate accounting.
- **Recommendation:** Add one private finalized-report aggregation pass owned
  by the report boundary, then project `AnalysisReportSummary` and the
  overlapping file/diagnostic counts from it. Keep evidence-step counts,
  rendered-trace counts, and phase-specific projection metrics in their
  operation-count owner; the goal is one traversal/definition for finalized
  file and diagnostic accounting, not one giant report object.
- **Guardrails:** Preserve the distinction between parse, file-level project,
  and report-level project diagnostics; keep summary computation cheap and
  deterministic; do not make the immutable public report store redundant
  mutable counters without an invariant owner.

**Fix Applied:** `FinalizedReportAggregate` now owns one private traversal of
the finalized file/diagnostic tree, including findings, diagnostic categories,
evidence steps, and rendered traces. `AnalysisReport::summary` and report
assembly project their views from that aggregate, while projection metrics
remain owned by the operation-count builder. Verified with `make fmt && make
ci`.

#### [ ] READ-007 — Correct the `SourceTable` ordering contract

- **Category:** ENCAPSULATE
- **Location:** `glass-lint-core/src/project/tables.rs:1-5, 13-50`
- **Impact:** Low — API / Documentation
- **Confidence:** High
- **Evidence:** The module documentation says the wrappers “preserve insertion
  order,” but `SourceTable` is a `BTreeMap`. Its `in_path_order` method returns
  the map iterator, and `module_ids` enumerates the same key order. The type
  therefore provides normalized path order, not insertion order; the input
  contract also rejects duplicate paths rather than retaining insertion data.
- **Why this is a readability issue:** Ordering determines traversal and stable
  `ModuleId` assignment, so the mismatch describes a different determinism
  guarantee than the implementation supplies. A future caller could preserve
  or test insertion order based on the module documentation and accidentally
  encode the wrong identity expectations.
- **Recommendation:** State the actual normalized path/BTree order in the
  module and method documentation, or change the abstraction to own insertion
  order if that is the intended contract. The lower-risk refactor is to make
  the path-order guarantee explicit and give the iterator a name that reflects
  it.
- **Guardrails:** Preserve duplicate detection, deterministic path traversal,
  `ModuleId` assignment, and the existing range/budget behavior. This finding
  does not recommend changing identity ordering without an explicit migration.

## Systemic Themes

- Phase ownership is generally present, but state transitions are often
  reconstructed from booleans, map membership, or positional arguments rather
  than represented by domain types.
- Public exports should distinguish caller-facing project inputs from
  crate-internal linked identities. Opaque internal types are not automatically
  useful public APIs merely because their definitions are public.
- Reporting has several deterministic aggregate contracts. Centralizing their
  semantic owners would make future schema additions safer without combining
  unrelated budget, timing, and presentation data.

## Decisions

- `analyze_source_at_path_with_observer` is intentionally a force-reanalysis
  path: an explicit source request may refresh an artifact, while executor
  scheduling skips completed paths. Keep those policies separate and name them
  rather than retaining a boolean mode.
- The current workspace has no downstream production caller for
  `LinkedModuleTarget` or `ModuleId`; narrow them to the linker boundary. Add a
  semantic read-only view only if a real consumer appears.
- Share one private finalized-report aggregation pass for file and diagnostic
  summary counts, but keep evidence/rendering and projection operation metrics
  separate because they have different owners and semantics.

## Coverage

Reviewed the complete Chunk 12 boundary in `CODEBASE_STRUCTURE_CORE.md`, the
root and core architecture/testing/contribution guidance, the session and
artifact implementations, project input/table types, report evidence and
operation types, and their direct callers. Earlier chunk reports were checked
to avoid repeating their findings; this report intentionally does not revisit
their matcher, flow, linker-session, catalog, compiler, parser, scheduler, or
rule-selection findings.

## Handoff

Chunk 12 is the final chunk. All twelve structure chunks now have a numbered
readability-audit report; no further chunk remains to hand off.
