# Codebase Readability Audit

## Summary

Chunk 06 (`analysis::matching`) has careful identity separation, typed
occurrence keys, lazy overlay/package iterators, and explicit constrained-match
fallbacks. The main readability and cost opportunities are in the query
adapter layers: one event view is immediately translated into a second
capability representation, and the evidence boundary re-materializes and
re-sorts selections that are already ordered in common paths.

## Findings

### [analysis/matching/query/view.rs]

#### [ ] READ-016 — Let `EventIndexView` resolve identities directly

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity/Architecture
- **Location:** `glass-lint-core/src/analysis/matching/query/view.rs:23-100,102-215,217-390`; `glass-lint-core/src/analysis/matching/query/mod.rs:124-132,195-240`

`OccurrenceIndexes::build_event_view` already constructs a closed enum whose
variants contain exactly the indexes valid for each `EventPredicate`. Its
`EventIndexView::resolve` method then immediately calls `capabilities()`, which
rebuilds the same variant matrix into `EventIndexCapabilities` using
`AnyIndex`, `LiteralIndex`, `ModuleIndex`, `RootedIndex`, and several `Option`
fields. Every identity resolution then traverses this second representation,
including its `Unsupported` arms. Adding or changing an event requires keeping
the source view, the capability conversion, and the generic resolver methods
consistent, while the intermediate type adds no ownership or validation beyond
the original enum.

**Recommendation:** Move the identity-resolution helpers onto
`EventIndexView` and match directly on its event variants, deleting
`EventIndexCapabilities` and the storage-shaped helper enums/structs. Keep the
existing typed event variants so unsupported identity/event combinations still
return `None`; preserve module overlay kinds, rooted global-object matching,
package predicates, literal private-network matching, and the exact fallback
boundary used by constrained roots. Add table-driven tests covering every
`EventPredicate`/`IdentityConstraint` combination that is currently supported
or rejected.

**Fix Applied:** None so far.

### [analysis/matching/occurrence.rs, analysis/matching/evidence.rs]

#### [ ] READ-017 — Avoid sorting and copying already ordered occurrence selections

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity/Performance
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:17-79,82-216,419-435,448-469`; `glass-lint-core/src/analysis/matching/mod.rs:378-390`; `glass-lint-core/src/analysis/matching/evidence.rs:16-37`; `glass-lint-core/src/analysis/matching/query/mod.rs:67-120`

`OccurrenceIndex::normalize` establishes `(FactId, span)` ordering for every
bucket, and `BorrowedOccurrenceIter` performs a deterministic k-way merge for
base and linked overlay buckets. Nevertheless, `OccurrenceSelection::into_ordered`
always collects a new `Vec<Occurrence>` and sorts it, including the direct
`Indexed` path and the already merged `Borrowed` path. `push_owned_evidence`
then passes that temporary vector to `EvidenceGroup::from_occurrences`, which
allocates a second vector of `ClassificationEvidenceOccurrence` values. Direct
indexed queries are common, so the normalized index’s ordering guarantee is
discarded at the evidence boundary and the operation is repeated before global
evidence normalization.

**Recommendation:** Represent ordering provenance on `OccurrenceSelection` or
provide separate ordered and unsorted materialization paths: preserve the
normalized `Indexed` and k-way-merged `Borrowed` iterators without a second
sort, while retaining sorting for `Scanned` and package selections whose base
and overlay buckets are concatenated. Let `EvidenceGroup` consume the selected
iterator into its final evidence representation, deleting the intermediate
`Vec<Occurrence>` where ordering is already guaranteed. Preserve duplicate
physical events for counts, deterministic tie-breaking, package/overlay
masking, empty-span filtering, and the later evidence truncation and group
sorting policies. Add equivalence tests for direct, linked-overlay, package,
and scanned selections.

**Fix Applied:** None so far.

## Systemic Themes

- The matcher has strong semantic newtypes and owner-local normalization; the
  main duplication is at adapters that translate an already constrained view
  into generic option storage.
- Ordering is correctly treated as a semantic output invariant, but the
  current boundaries enforce it more than once. Any simplification must keep
  the distinction between normalized single buckets, merged overlays, and
  concatenated package scans.
- Constrained matching’s indexed/fallback split is intentionally retained. The
  findings target representation and transport overhead, not the bounded
  fallback policy or certainty behavior.

## Open Questions

- Should `OccurrenceSelection` expose an explicit `is_normalized`/ordered
  contract, or should each constructor return a distinct selection type so
  future query code cannot accidentally bypass ordering guarantees?
- Can the evidence API accept a borrowed/streaming occurrence source while
  preserving its exact total count and truncation markers, or should only the
  intermediate `Occurrence` vector be removed after a pre-count pass?

## Coverage

Reviewed Chunk 06: occurrence indexes and normalization; call/member/
construction/literal index owners; fact-to-index projection; module identity
maps and linked overlays; lazy package and k-way occurrence iterators; query
views and identity resolution; direct and constrained evidence publication;
argument matcher artifacts, overlay contexts, constrained indexed/fallback
evaluation, effective identity resolution, and evidence accumulation/presentation.
Read the root/core architecture, testing/contributing guidance, the complete
readability-audit skill instructions, and existing audits 001–005. No source
or test files were changed.
