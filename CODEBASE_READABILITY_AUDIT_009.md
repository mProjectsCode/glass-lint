# Codebase Readability Audit

## Summary

Chunk 09 has strong phase ownership: declaration-shaped queries are
normalized, same-event contradictions are detected before physical planning,
requirements are derived from executable roots, and runtime matching consumes
compiler-owned plans. The main opportunities are at those phase seams. One
event vocabulary is copied into a second compiler enum without semantic
lowering, validation is repeated at several trusted internal boundaries, and
classification/evidence invariants are represented by duplicated constructors
and a debug-only capacity check.

## Findings

### [api/compiler/mod.rs and api/compiler/physical.rs]

#### [ ] READ-022 — Remove the one-to-one event predicate vocabulary

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:119-187`; `glass-lint-core/src/api/compiler/normalized.rs:199-228`; `glass-lint-core/src/api/compiler/physical.rs:42-70,515-566`

`EventPredicate` duplicates every `EventSpec` variant and payload, and
`lower_event` performs only a field-for-field clone. The normalized IR already
stores `EventSpec` directly in `NormalizedEvent`; physical planning then
converts it to `EventPredicate`, after which matcher preparation and runtime
evaluation carry the duplicate type through `PhysicalRoot` and evaluator
views. A new event kind therefore requires synchronized edits to the
declaration enum, compiler enum, lowering match, physical validation, and
runtime matches, while the conversion itself establishes no new invariant.
`IdentityConstraint` is a different case because it deliberately maps
heuristic identities to `Any` and changes some payload types.

**Recommendation:** Evaluate making the compiler-owned physical roots carry
`EventSpec` directly, or centralize the event representation behind one
compiler-owned semantic type with a single conversion boundary. Keep
`IdentityConstraint` separate unless its heuristic normalization is also
explicitly represented. Delete the redundant event enum and `lower_event`
mapping only after checking all matcher index dispatch and diagnostics. Guard
the change with plan-shape, member-path, event-kind, and runtime matching tests;
preserve the distinction between calls, reads, writes, imports, strings,
constructors, and classes, including deterministic ordering and physical
root equality.

**Fix Applied:** None so far.

### [api/compiler/validate, api/compiler/normalize.rs, and api/compiler/physical.rs]

#### [ ] READ-023 — Assign validation to one authoritative compiler boundary

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/api/compiler/mod.rs:202-221`; `glass-lint-core/src/api/compiler/validate/pass1_3.rs:8-34`; `glass-lint-core/src/api/compiler/validate/pass4_10.rs:60-120`; `glass-lint-core/src/api/compiler/normalize.rs:35-142`; `glass-lint-core/src/api/compiler/physical.rs:291-309,584-603`

One catalog compilation path validates the same query through several
successive layers: `compile_queries` calls `validate_query_decl`, normalization
calls `validate_normalized`, and `PhysicalPlan::from_roots` validates every
physical root again. The checks overlap in concrete ways: identity/event
compatibility is checked by `validate_event_query`, contradiction/subject
validation, and normalized-root validation; lifecycle source validity is
checked before and after normalization; root count and root well-formedness
are bounded both by `RootBudget` during planning and by `from_roots` at the
sealing boundary. The comments explicitly describe some later checks as
conditions that “should have been rejected at construction”. This makes error
ownership harder to follow and repeats traversal, matching, and bound checks
for every compiled rule.

**Recommendation:** Define one authoritative semantic validation pass for
authored declarations, one normalization invariant checker for the normalized
IR, and one physical-plan sealing validator for callers that can construct
physical roots independently. Make the production compiler avoid repeating
the same full semantic checks after normalization has established them, but
retain physical sealing checks because `PhysicalPlan::from_roots` is an
independent safety boundary. Use private validated constructors or a token only
where that removes a real repeated traversal; do not weaken test-facing
invalid-plan validation. Preserve stable diagnostic precedence, malformed
internal input handling, root and lifecycle bounds, contradiction detection,
exact requirements derivation, and tests that inject invalid normalized or
physical values.

**Fix Applied:** None so far.

### [api/classification.rs]

#### [ ] READ-024 — Centralize classification evidence construction

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/api/classification.rs:124-179`; representative callers `glass-lint-core/src/analysis/matching/evidence.rs:19-40,88-100`, `glass-lint-core/src/analysis/flow/cross/evidence.rs:239-249`, and `glass-lint-core/src/analysis/flow/projector/evidence.rs:244-255`

`ClassificationEvidence::from_occurrences`, `from_occurrence`, and
`with_total_count` each assemble the same evidence fields and establish the
same count/occurrence relationship. The first derives count from the vector,
the second special-cases one occurrence, and the third accepts an externally
aggregated count; only emptiness and total-count validation differ. Keeping
three object-construction paths makes future fields such as certainty,
truncation, or occurrence ordering easy to initialize inconsistently. The
callers already split between grouped matching evidence and direct/aggregated
test paths, so the duplication is at the invariant owner rather than the
matching policy.

**Recommendation:** Add one private constructor that accepts the occurrence
list plus an explicit total count and truncation state, validates
`total_count >= occurrences.len()`, and derives the one-occurrence case by
delegation. Keep small named wrappers only where they improve call-site
meaning, and delete their repeated struct literals. Preserve the empty-group
contract, saturating `usize`→`u32` conversion, certainty, truncation defaults,
and all evidence ordering/merge behavior. Add tests for empty groups, a single
occurrence, aggregated totals, and totals smaller than retained occurrences.

**Fix Applied:** None so far.

### [api/classification.rs]

#### [ ] READ-025 — Enforce evidence-capacity equality in release builds

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/api/classification.rs:219-295`; representative caller `glass-lint-core/src/analysis/project/projection.rs:591-603`

`RuleEvidenceTable::merge_equal_capacity` relies on
`debug_assert_eq!(self.capacity, other.capacity)` and then moves all of the
other table's entries into `self`. The method's name and the private
`RuleEvidenceCapacity` type state an invariant, but release builds accept a
mismatched table. A cross-module projection currently passes tables built from
the same `ProjectionPlan` capacity, yet a future merge path, selection change,
or error recovery could combine different catalogs and silently retain indexes
that are outside the destination capacity. The later `items_mut` checks only
new writes; it does not validate entries imported by the merge.

**Recommendation:** Make the capacity invariant explicit at the merge owner:
return a structured `RuleEvidenceError` on mismatch, or carry a shared
catalog-capacity token so tables with different capacities cannot be merged.
Remove the debug-only assertion after the runtime guard is in place, and keep
the existing same-capacity fast path. Preserve deterministic rule-index
ordering, out-of-range errors for recording/replacement, cross-module evidence
merging, and the empty-table behavior. Add a release-mode test that attempts a
different-capacity merge and verifies it is rejected without modifying the
destination.

**Fix Applied:** None so far.

## Systemic Themes

- Compiler IR types are useful when they normalize semantics or protect a
  phase boundary; exact one-to-one mirrors should carry a distinct invariant
  or be removed.
- Runtime plans should be sealed by one clear owner. Rechecking an invariant
  at a boundary is valuable when inputs are independent, but the production
  path should not repeatedly perform the same full validation after a private
  token has established it.
- Evidence tables and evidence values have meaningful bounded invariants.
  Those invariants should be represented by constructors and runtime errors,
  not only debug assertions or repeated struct literals.

## Review Resolutions

- `EventSpec` and `EventPredicate` are currently one-to-one. READ-022 may remove
  the mirror only if the physical root can carry the declaration event without
  exposing authoring concerns; otherwise keep one compiler-owned event type
  and centralize the conversion rather than adding another mirror.
- `PhysicalPlan::from_roots` is the production sealing boundary; the
  test-facing constructors deliberately exercise malformed physical values.
  READ-023 must remove only redundant production validation, not this boundary
  check or its tests.
- Do not introduce a shared catalog-capacity token across unrelated owners.
  READ-025 needs a runtime equality error (or an equivalent owner-local
  invariant), not cross-coupled capacity types.

## Coverage

Reviewed Chunk 09: classification evidence and rule indexes; compiler entry
points; event/identity lowering; normalized query IR; canonical argument
constraints; same-event merge and contradiction detection; object-flow
compilation; physical roots, budgets, requirements, and plan validation;
compiler catalog/rule records; validation passes and diagnostics; and the
test-only logical/physical reference seam. Read the root/core architecture,
testing/contributing guidance, the complete readability-audit skill
instructions, and existing audits 001–008. No source or test files were
changed.
