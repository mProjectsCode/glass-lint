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

### Query-view layering

No finding: `EventIndexCapabilities` is a private, stack-only dispatch view.
Although its option fields represent unsupported combinations, the type
centralizes the shared identity operations and does not allocate or leak
matcher storage. Replacing it with per-event helpers would move the same
dispatch policy rather than remove a concrete ownership or maintenance cost.

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
  phase transitions, or validated state. The project context and constrained
  evaluation inputs meet that bar; no single-use parameter-object finding is
  retained here.
- Query abstractions should encode supported combinations once. The current
  private capability view is an intentional shared dispatch boundary; its
  unsupported enum variants are internal fail-closed cases, not leaked API
  states.
- Structure inventories are part of the internal API story: removed types
  should disappear from them in the same migration that removes their code.

## Open Questions

- `OccurrenceSelection` is crate-private, so the scanned variant can change
  internally while retaining the same evidence ordering and duplicate-event
  behavior. Keep the event-specific overlay kinds and the existing project
  context lifetime; neither is an open design question.

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
