# Codebase Readability Audit — Chunk 17

## Summary

Chunk 17 owns the classification result values, bounded rule-evidence table,
compiled matcher IR, normalized lifecycle/object-flow types, physical plan
roots, and compiler error/selection boundaries. The compiler phase outline is
clear and the private plan storage is a good boundary. The main readability
risks are semantic contracts that remain writable after construction, late
validation of raw physical handles, partial normalization state, and error
types that mix authored-query failures with compiler bugs. Lifecycle readiness
also has multiple semantic interpreters, so a mode change would require
coordinated edits in execution and the reference evaluator.

The physical-plan, requirements, compatibility-matrix, and canonical-argument
findings from Chunk 15 were checked first and are not repeated. The evidence
reservation/finalization findings from Chunks 7 and 8, the public trace-handle
finding from Chunk 6, and the catalog identity finding from Chunk 16 are also
kept separate; the findings below focus on the classification/compiler type
contracts themselves.

## Findings

### Classification values and evidence storage

#### [ ] READ-001 — Make classification evidence own its count and truncation invariant

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/classification.rs:45-84,108-210`; constructors in `analysis/matching/mod.rs:336-368`, `analysis/matching/evidence.rs:120-131`, `analysis/flow/cross/evidence.rs:281-297`, and `analysis/flow/projector/evidence.rs:241-255`

`ClassificationEvidence`, `ClassificationEvidenceOccurrence`, and
`MatchedCapability` expose all of their fields publicly. In particular,
`count`, `truncated`, and `occurrences` describe one bounded-evidence
contract, but callers can construct any combination. Internal producers use
several conventions: local grouping sets `count` from the retained
occurrences, matching evidence uses a total count that can exceed the
retained list, and flow evidence writes a single occurrence before later
truncation passes update the flag. The type does not state or enforce when
`count == occurrences.len()` is required, when `count` is a saturated total,
or which operation is allowed to mark an item truncated.

This makes serialized classification correctness depend on every producer
remembering the same bounded-report semantics. A new producer can emit an
under-counted item, mark a non-truncated item with omitted occurrences, or
forget to preserve total count while still producing a valid Rust value.
`RuleEvidenceTable::record`, `extend`, and `replace` accept those values
without a validation boundary.

**Recommendation:** Keep the report structs readable from the outside but
make their storage private and provide invariant-owning constructors such as
`from_occurrences` and an explicit `with_total_count`/truncation operation.
Give `RuleEvidenceTable` a narrow grouped/admit/finalize API instead of raw
record/replace paths, and delete direct struct literals after migration.
Preserve total-match versus retained-occurrence semantics, saturating counts,
event-level truncation, certainty downgrades, deterministic occurrence order,
and empty-group suppression. Keep the trace-head field private as part of the
trace-handle boundary identified in Chunk 6.

**Fix Applied:** None so far.

### Compiler error and phase boundaries

#### [ ] READ-002 — Separate authored query diagnostics from compiler invariant failures

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/validate/error.rs:5-55,57-73`; normalization callers in `api/compiler/normalize.rs:67-90` and `api/compiler/normalize_all.rs:104-124,253-263`; mapping in `api/compiler/mod.rs:216-245`; public error boundary in `api/rule/error.rs:31-59`

`QueryCompileError` contains both authored-input cases such as
`MissingBinding`, `ContradictoryPredicate`, and `InvalidLifecycle`, and the
internal-only `InternalInvariant { detail: String }` case. The normalized
validator and same-event merger emit the latter for malformed compiler IR.
However, `compile_queries` maps every `QueryCompileError` through
`diagnostic_name()` into `MatcherBuildError::QueryCompileError`, and the
catalog then classifies that as `CompiledCatalogError::InvalidQuery`. A
compiler bug such as non-dense normalized slots or missing normalized
identity is therefore presented to callers as an authored query diagnostic
with a stable user-facing code. Physical-plan failures take the neighboring
generic `InvalidLoweredQuery(String)` path, losing their typed distinction as
well.

The comments on `QueryCompileError` promise that internal bugs are separate,
but the conversion path does not preserve that separation. This makes error
handling, telemetry, and future recovery policy depend on diagnostic strings;
it can also encourage rule authors to “fix” declarations that were never
invalid.

**Recommendation:** Split the compiler result into a user-facing
`QueryValidationError` and an internal `CompilerInvariantError` (and retain a
typed physical-plan validation error). Map only the former to
`QueryDiagnostic`; propagate the latter as an internal catalog/compiler
failure with a diagnostic-safe message and no authored-query code. Replace
the free-form invariant detail where possible with private invariant variants
or contextual source information. Preserve stable authored diagnostic codes,
fail-closed catalog compilation, no panics on malformed internal state, and
the existing distinction between invalid declarations and invalid lowered
plans.

**Fix Applied:** None so far.

#### [ ] READ-003 — Give physical roots typed object-slot and relation constructors

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/physical.rs:39-168`; root construction in `api/compiler/physical.rs:359-421`; object-slot conversion in `api/compiler/normalize_all.rs:127-129,173-187`

`PhysicalRoot::ReturnedSubject` and `InstanceSubject` encode an object
correlation slot as a raw `u32`. The invalid value `u32::MAX` is rejected only
inside `PhysicalRoot::validate`, after the enum has already been constructed.
The same root validator also repeats variant-specific checks for nonempty
members, producer identity kinds, and event/member agreement. The planner can
construct a default empty `SymbolPath` for unsupported subject/event shapes,
leaving another invalid root to be rejected later. `PhysicalPlan::try_new`
is the only normal sealing boundary, while test-only `PhysicalPlan::new` and
crate-visible enum fields make the intermediate invalid states available to
compiler tests and neighboring modules.

The raw sentinel and late matrix checks make physical plan correctness a
property of a distant validator rather than of the root value. Adding a new
subject relation requires updating enum construction, sentinel validation,
event compatibility, and plan tests; an omitted update can become a generic
lowering failure or, if a check is missed, an executable root with an
unusable slot.

**Recommendation:** Introduce a private validated `ObjectSlot`/root handle
with a bounded constructor, and provide relation-specific constructors for
direct scans, returned-member roots, constructed-member roots, and lifecycle
roots. Have constructors reject unsupported event/member combinations and
empty evidence before returning a `PhysicalRoot`; make fields private and
remove the sentinel plus ordinary-path construction of empty members. Keep
`PhysicalPlan::try_new` as a defensive whole-plan check, retain bounded slot
renumbering and deterministic root ordering, and preserve the compatibility
and requirement checks already identified in Chunk 15.

**Fix Applied:** None so far.

### Normalization and lifecycle semantics

#### [ ] READ-004 — Seal same-event merging through a phase-aware accumulator

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Lifecycle
- **Location:** `glass-lint-core/src/api/compiler/normalize_all.rs:104-124,131-275`

`SameEventMerge` is an open mutable record containing five independent pieces
of state: the event variable, optional event kind, optional identity, optional
subject, and a raw constraint vector. `merge_event`, `merge_predicate`, and
`merge_member_subject` can be called in any order; only `finish` discovers
that event kind or identity was never supplied and reports an
`InternalInvariant`. `merge_predicate` and `finish` both receive `event_var`
even though the accumulator already stores it, and the member-subject path
must compare the caller-provided variable before checking the stored subject.
The accumulator therefore exposes both construction sequencing and duplicate
correlation identity to its caller.

This is the central state machine for order-independent same-event `All`
normalization. Its partial options and duplicated variable argument make the
required invariants hard to see, and a future merge operation can forget to
participate in final validation or use a variable different from the one that
created the accumulator. The resulting failure is late and categorized as an
internal string rather than as a local merge-state error.

**Recommendation:** Give the merger a private phase-aware API: retain the
event variable only in the accumulator, expose merge operations that accept
only branch data, and finish through a typed state that requires event kind
and identity before producing `NormalizedEvent`. Keep subject compatibility,
contradiction detection, argument canonicalization, branch-order
independence, and uncorrelated-conjunction diagnostics in that owner. Delete
the duplicate `event_var` parameters and the open-ended `Option` state after
callers migrate; use a separate explicit error for an impossible compiler
state rather than a generic invariant string.

**Fix Applied:** None so far.

#### [ ] READ-005 — Give lifecycle modes one semantic owner across execution and reference evaluation

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/object_flow.rs:17-163,251-289`; runtime readiness in `analysis/model/flow.rs:431-446`; reference interpretation in `api/compiler/reference.rs:230-304`

`CompiledObjectFlow` stores `RequirementMode` and `CompletionMode`, and its
`requirements_ready` method interprets the first mode from a caller-supplied
completed count. Runtime `LifecycleEvidence` separately interprets the same
mode while deciding readiness, while the test-only reference evaluator
rebuilds the mode semantics as `condition_sets` and
`completion_candidates`. The reference plan copies the mode and lowers sinks
into yet another matcher shape. These are independent implementations of
“any/all/configuration” semantics, not merely different presentations of the
compiled flow.

The count-based API also makes the compiled type trust the caller to provide
the right domain count; it cannot distinguish a set containing an invalid or
duplicate lifecycle index from a complete set. A new completion mode, empty
condition rule, or bounded-index policy must therefore be updated in the
compiled flow, runtime state, and reference evaluator, with no shared semantic
operation identifying omissions.

**Recommendation:** Make lifecycle condition/completion semantics a private
compiled capability owned by `CompiledObjectFlow`, exposing operations over
validated requirement/sink evidence rather than a raw count. Have runtime
readiness and the reference evaluator consume that capability or a shared
immutable mode description while retaining an independent reference matcher
for event selection. Replace raw `Vec<usize>` sink indices with validated
bounded argument handles, and keep filtering of absent arguments explicit at
the execution boundary. Preserve any/all/configuration behavior, path-local
evidence, duplicate-event handling, reference-test independence where it
detects semantic drift, and all lifecycle limits.

**Fix Applied:** None so far.

## Systemic Themes

- The compiler has private plan storage, but several values become
  crate/publicly writable before their invariants are sealed. Constructors and
  domain handles should carry count, slot, evidence, and lifecycle guarantees
  into the next phase.
- Error ownership follows the same boundary problem: authored diagnostics,
  physical validation failures, and compiler bugs are represented as adjacent
  strings or one enum even though callers need different responses.
- Lifecycle modes and evidence semantics are deterministic and bounded, but
  their meaning is repeated across runtime and reference paths. A shared
  semantic description should reduce drift without making the reference
  evaluator reuse the production event-search implementation.

## Open Questions

- Should external callers be able to construct classification results, or are
  those values read-only outputs? The answer determines whether constructors
  are public or only report-assembly-facing.
- Should an internal compiler invariant be surfaced as a structured build
  failure, logged as an implementation error, or both? The authored-query
  diagnostic namespace should remain reserved for declaration failures.
- Is the reference evaluator required to remain intentionally independent of
  production readiness logic? If so, share only validated lifecycle mode data
  and add an explicit parity test rather than sharing the matcher algorithm.

## Coverage

- **Reviewed modules:** `api::classification`; `api::compiler::{mod,rule,
  normalize,normalize_all,normalized,object_flow,physical,requirements,
  error,validate::error}`; `api::rule::{error,query::error}`.
- **Workflow traced:** rule selection → query validation and normalization →
  physical root construction and plan sealing → local/cross-flow readiness →
  classification evidence accumulation and report assembly, including the
  reference lifecycle evaluator.
- **Prior overlap check:** Normalized requirements, identity/event/subject
  compatibility, canonical argument flattening, catalog identity, trace
  handles, and flow evidence reservation/finalization were compared with
  Chunks 6, 7, 8, 13, 15, and 16 and not repeated as the same finding.
- **Verification:** Read-only audit; no source or test changes were made.

