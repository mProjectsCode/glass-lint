# Codebase Readability Audit — Chunk 06

## Summary

Chunk 06 owns matcher occurrence indexes, project-linked occurrence overlays,
query selection, argument-constrained evaluation, and evidence publication.
The typed occurrence keys, lazy borrowed overlay iterators, centralized
normalization, and effective-identity precedence are well aligned with the
matching architecture. The findings below focus on avoidable ownership hops,
temporary matcher bundles, and one generic query facade that is broader than
the event views it serves. The chunk inventory also contains one stale type
entry that should be reconciled.

## Findings

### Occurrence selection and evidence

#### [ ] READ-024 — Scanned occurrence selections are recopied at the evidence boundary

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:23-50,404-420`; `glass-lint-core/src/analysis/matching/query/mod.rs:148-186`; `glass-lint-core/src/analysis/matching/query/view.rs:327-364`

`OccurrenceIndex::matching` eagerly collects every matching bucket into a
`Vec<Occurrence>`, then `OccurrenceSelection::scanned` immediately converts
that vector into an `IntoIter`. At the evidence boundary,
`OccurrenceSelection::into_ordered` collects every selection into a second
vector before sorting it. Returned-member, instance-member, rooted-path, and
literal predicate queries use this path, so each eager scan pays for two
owned occurrence buffers even though the first buffer already contains all
the values that need ordering.

**Recommendation:** Keep the lazy borrowed variants unchanged, but let the
scanned variant retain the owned vector (or provide an owner-level
`scanned_ordered` conversion) and sort that allocation in place. Preserve the
current event/span ordering, duplicate physical events, and `None` result for
an empty match; only the intermediate `IntoIter` and second collection should
disappear.

**Fix Applied:** None so far.

#### [ ] READ-025 — Constrained-root preparation forwards a temporary root bundle

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:27-36,55-74,133-152`

`ConstrainedEvaluation::prepare` creates a `ConstrainedRoot` in
`filter_map`, immediately borrows it for `PreparedConstrainedRoot::new`, and
then drops it. The constructor copies all five references and the rule index
field into another `ConstrainedRoot` stored inside the prepared value. The
temporary has no independent consumer or invariant; it exists only to bridge
two adjacent iterator stages.

**Recommendation:** Construct `PreparedConstrainedRoot` directly from the
matched `PhysicalRoot` fields and `input.rule_index`, or give it a
`from_constrained_scan` constructor that performs preparation in one step.
Retain the prepared path cache, indexed/fallback/published state, and the
same physical-root filtering; remove only the field-forwarding temporary.

**Fix Applied:** None so far.

### Constrained evaluation API

#### [ ] READ-026 — `MatcherEvaluationContext` is an immediately destructured parameter object

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Internal API Design
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:291-295,315-375,1026-1044`

`try_compute_constrained_evidence` constructs a private
`MatcherEvaluationContext` containing three values, and
`compute_constrained_inner` immediately destructures all three fields before
doing any work. The test helper constructs the same bundle directly. The
type has no methods, validation, or ownership transition, and the
artifact/project association that matters to production is already grouped
by `MatcherProjectContext` before this call.

**Recommendation:** Pass the artifact, project overlay, and mutable
operation counter as explicit private parameters, or destructure the bundle
at the sole construction boundary and call an inner function with those
values. Keep the lifetime relationships and operation accounting explicit;
the simplification should remove only this single-use forwarding type.

**Fix Applied:** None so far.

### Query-view layering

#### [ ] READ-027 — Each event query builds a broad capability facade with unsupported slots

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Internal API Design
- **Location:** `glass-lint-core/src/analysis/matching/query/view.rs:23-100,102-245`

`EventIndexView` already stores the exact indexes and event-specific policy
for each event. Every `resolve` call then constructs a fresh
`EventIndexCapabilities`, which contains `AnyIndex`, optional global/member/
module/rooted wrappers, and a `LiteralIndex`; most of those fields are
`Unsupported` or `None` for the selected event. Resolution is immediately
delegated through that temporary facade. This creates a second capability
model, several impossible states, and repeated translation code between the
event enum and the generic option bag without persisting a reusable object.

**Recommendation:** Move the identity-resolution dispatch onto
`EventIndexView`, using small private helpers for shared module, rooted, and
literal operations, or replace the broad facade with a narrower event
capability trait/object that cannot represent unsupported combinations. Keep
the existing fail-closed `None` behavior, overlay policy, package matching,
and explicit event-to-index mapping; the goal is to remove the unused
capability storage layer rather than merge semantically different indexes.

**Fix Applied:** None so far.

### Structure/API inventory

#### [ ] READ-028 — The chunk inventory names a matcher input type that no longer exists

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Documentation/API
- **Location:** `CODEBASE_STRUCTURE_CORE.md:459-469`; current implementations in `glass-lint-core/src/analysis/matching/arguments/mod.rs:237-284`

The structure inventory lists
`matching::arguments::MatcherProjectInputs`, described as supplying linked
identity and call-result overlays. No source file defines or references that
type. The current boundary is split between `MatcherProjectOverlay` and
`MatcherProjectContext`, with the projection caller constructing the overlay
and passing it into `MatcherProjectContext::from_facts`. Leaving the removed
name in the inventory makes the documented matcher API disagree with the
actual internal API and can send future audits or contributors toward a dead
abstraction.

**Recommendation:** Remove the stale entry or replace it with the current
`MatcherProjectOverlay`/`MatcherProjectContext` ownership description after
confirming that the inventory is intended to be authoritative. Do not add a
compatibility type solely to satisfy the document; preserve the current
project-overlay lifetime and artifact-pairing guarantees.

**Fix Applied:** None so far.

## Systemic Themes

- An owned selection should have one ownership transition. The eager scan
  path can sort its existing vector, while borrowed overlay paths can retain
  their lazy merge and final ordering boundary.
- Private matcher bundles are valuable when they enforce artifact identity,
  phase transitions, or validated state. Single-use structs that only forward
  fields between adjacent functions should be folded into the owning
  constructor or call.
- Query abstractions should encode supported combinations once. A generic
  option bag with explicit unsupported variants obscures the event-specific
  contract and increases the surface that must remain synchronized.
- Structure inventories are part of the internal API story: removed types
  should disappear from them in the same migration that removes their code.

## Open Questions

- Before changing `OccurrenceSelection`, confirm whether any caller relies on
  `Scanned` being an iterator rather than an owned collection; the safe
  migration can keep the enum private and change only its evidence-boundary
  conversion.
- If the query view is simplified, retain separate module overlay kinds for
  calls, member calls, member reads, classes, and constructors. They encode
  policy, not merely storage shape.
- If `MatcherEvaluationContext` is removed, keep the project overlay passed
  by value or otherwise tied to the artifact lifetime so argument evaluation
  cannot accidentally use identities from another project artifact.

## Coverage

Reviewed the chunk-06 structure entries and their implementation/test support:

- `analysis/matching/{mod,build,evidence,identity_map,indexes,occurrence}.rs`
- `analysis/matching/arguments/{mod,evaluator,identity}.rs`
- `analysis/matching/query/{mod,view}.rs`
- Related project projection and identity callers in
  `analysis/project/{projection,identities}.rs`
- The structure inventory was checked against source references for all
  matcher argument and query-view types.

No source, test, configuration, dependency, or other documentation files
were changed by this audit.
