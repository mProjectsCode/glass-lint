# Codebase Readability Audit — Chunk 10

## Summary

Chunk 10 covers occurrence indexes, linked module overlays, constrained
argument evaluation, candidate iterators, query views, and evidence
accumulation. The matching layer has several good ownership choices: typed
occurrence keys, a single fact-to-index projection, explicit identity
overlays, bounded fallback scans, and deterministic k-way occurrence merges.

The main risks are mismatched or duplicated matching protocols. Constrained
member evaluation does not use the same effective-argument path as call
evaluation, overlay/index relationships are carried by independent arguments,
normalization depends on a collector ordering promise, and one lookup strategy
is mirrored in two enums. These issues affect semantic identity and
deterministic evidence, not just code shape.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Constrained argument evaluation

#### [x] READ-051 — Apply wrapper-effective arguments to member constraints

- **Severity:** High
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Matching / Semantic identity / wrapper handling
- **Location:** `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:122-180,262-271`,
  `analysis/flow/effect/mod.rs:257-264`

`MatcherEvaluator::fact_matches_clause` handles wrapper-effective arguments
only in the `EventPredicate::Call` branch through `check_constrained_args`.
The `EventPredicate::MemberCall` branch sends the raw `args` slice directly to
`constraints_match`, even though `CallUnwrap` carries the canonical target
argument list for `.call()` and `.apply()`. The same fact can therefore match
call constraints against one argument view and member-call constraints against
the wrapper syntax view. This extends the raw/effective mismatch identified in
Chunk 9's summary sink path into the primary constrained matcher.

Route both branches through one effective-argument accessor, preferably a
fact/call view that owns wrapper unwrapping, and remove the branch-specific
raw slice choice. Preserve member identity matching, receiver removal for
`.call()`, array expansion for `.apply()`, missing/dynamic argument
fail-closed behavior, and deterministic operation accounting.

**Fix Applied:** Routed member-call argument constraints through the same
effective `.call()`/`.apply()` argument view used by ordinary calls. Added a
regression covering receiver removal, array expansion, and dynamic rejection
for member-call wrappers. Verified with
`cargo test -p glass-lint-core --test integration matching::declarative::arguments`
and `make fmt && make ci`.

### Matcher artifact and overlay identity

#### [x] READ-052 — Bind matcher evaluation to its occurrence-index artifact

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Identity ownership / overlay
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:43-84`,
  `analysis/matching/mod.rs:42-51,64-106`,
  `analysis/project/projection.rs:457-493`

`MatcherEvaluationContext` carries a fact stream, occurrence indexes, optional
linked occurrence view, module identities, and result identities as separate
references. `LinkedOccurrenceView` borrows occurrence slices from the index it
was built from, but the constrained-evidence entry point can receive that view
alongside a different stream or base index. The normal project path pairs them
correctly, yet the type boundary does not express the pairing of FactIds,
ValueIds, names, and borrowed overlay buckets. A mismatched internal caller
could evaluate a valid-looking occurrence against another artifact's facts or
identity map and produce evidence at the wrong semantic event.

Introduce an artifact-bound matcher input/view that owns or borrows the stream,
base indexes, and any overlay built from that same index; keep project identity
maps and result overlays as explicitly qualified additions. Delete the raw
parallel context arguments after migration. Preserve zero-copy borrowed
overlays, local versus linked identity precedence, bounded fallback evaluation,
and the rule that cross-artifact IDs cannot establish evidence.

**Fix Applied:** Replaced the separate local matcher stream/index inputs and
occurrence overlay with `MatcherArtifact`, which is built from one
`SemanticFacts` artifact and retains its linked occurrence view. Project
identity maps and call-result identities remain a separate qualified overlay;
ordinary indexed evidence and constrained evaluation now consume the same
artifact-bound index and stream.

**Verification:** `cargo test -p glass-lint-core analysis::matching --lib`
(42 passed); `make fmt && make ci` (passed).

### Occurrence storage and iteration

#### [x] READ-053 — Make occurrence normalization enforce its ordering contract

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Collection invariant / Determinism
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:370-447`,
  `analysis/matching/build.rs:26-35`

`OccurrenceIndex::normalize` only calls `dedup_by_key`; it does not sort each
bucket. Its correctness and deterministic merge order therefore depend on the
implicit promise that every `push_occurrence` caller inserts monotonically by
`(FactId, span)`. The current fact builder satisfies that promise by traversing
the stream in order, but `OccurrenceIndex` itself exposes insertion and
normalization as separate operations to the rest of the matching module. A new
collector, overlay source, or reordering transform can create separated
duplicates or unsorted buckets while `normalize_occurrences` reports a
completed index and merge iterators assume sorted slices.

Move sorting and deduplication into the owner (or make the precondition a
private construction phase that cannot be queried before sealing). Preserve
the current event/span/bucket tie-break order, zero-copy indexed queries after
normalization, and deterministic evidence output; do not rely on a caller
comment as the only guard for the invariant.

**Fix Applied:** `OccurrenceIndex::normalize` now sorts each bucket by the
documented `(FactId, span start, span end)` key before deduplicating. The
owner therefore enforces deterministic normalized order even when a collector
inserts out of order; the regression test now exercises that ordering.

**Verification:** `cargo test -p glass-lint-core --lib analysis::matching`
and `make fmt && make ci` pass.

#### [x] READ-054 — Replace mirrored candidate-collection enums with one owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** API / Iterator ownership
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:17-63`

`CandidateOccurrences` and `CandidateOccurrenceIter` encode the same four
lookup strategies—indexed, borrowed merge, borrowed package, and scanned—and
`IntoIterator` manually maps every producer variant to the matching iterator
variant. Adding a lookup strategy requires changing both enums, the conversion,
and `Iterator::next`; an omitted arm is a compile error but the representation
still spreads one protocol across two owners and makes borrowing/ownership
semantics harder to follow.

Make the collection enum own its iterator conversion through a single private
iterator representation, or define one strategy enum with the appropriate
borrowed/owned payloads and expose a narrow iterator adapter. Preserve
zero-allocation indexed and merged paths, owned scanned results, package
overlay laziness, and deterministic occurrence ordering.

**Fix Applied:** `CandidateOccurrences` now stores the iterator state directly
and implements `Iterator`, with constructors for indexed slices and scanned
vectors. Removed the mirrored `CandidateOccurrenceIter` enum and conversion
match; borrowed merge and package iterators remain lazy and allocation-free.

**Verification:** `cargo test -p glass-lint-core analysis::matching --lib`
(42 passed) and `make fmt && make ci` (all passed).

### Linked overlay dispatch

#### [x] READ-055 — Give `ModuleOverlayKind` one bucket-access API

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** API / Internal dispatch
- **Location:** `glass-lint-core/src/analysis/matching/mod.rs:160-171,222-233`

`LinkedOccurrenceView` has two exhaustive `ModuleOverlayKind` matches that map
the same five variants to the same five maps: `module_buckets_mut` and
`module_buckets`. The enum's mapping is therefore duplicated solely because
one caller needs mutable access during remapping and the others need shared
access during lookup. A new overlay kind must update both matches, and a
partial update can make build and query behavior diverge even though the
compiler only reports the missing match when a variant is added rather than
when an existing mapping is accidentally changed.

Move the mapping to one view-owned accessor pattern or a private table bundle
that supplies shared and mutable access through a single owner method. Preserve
the distinct call/member/class/constructor buckets, masking semantics,
global-call promotion, and borrowed overlay lifetimes.

**Fix Applied:** The current `ModuleOccurrenceOverlay` already owns one
`BTreeMap<ModuleOverlayKind, BorrowedModuleBuckets>` and routes mutable and
shared access through `buckets_mut` and `buckets`; no duplicated
variant-to-bucket mapping remains. Marked this stale finding as addressed.

**Verification:** `make fmt && make ci` (passed).

## Systemic Themes

Chunk 10's typed occurrence and overlay structures make deterministic matching
possible, but key relationships remain caller protocols: wrapper-effective
arguments, stream/index/overlay identity, occurrence insertion order, and
lookup strategy ownership. These should be represented by one canonical view
or sealing transition so future matcher additions cannot silently diverge
between local, linked, and constrained paths.

Refactors must retain the current fail-closed behavior for unsupported or
dynamic values, preserve independent candidate occurrences, keep overlay
masking and identity precedence explicit, and avoid turning a borrowed fast
path into an unbounded allocation.

Search signals used for this chunk included raw argument slices beside unwrap
effective arguments, independently passed matching artifacts, normalization
without sorting, mirrored candidate enums, and duplicate overlay-kind maps.

## Open Questions

- The effective-argument accessor should be shared with flow summaries and
  projection without exposing flow-specific state to the matcher layer.
- An artifact-bound matcher input should keep linked identity maps optional and
  preserve the existing ability to evaluate local indexes without a project
  overlay.
- The next unreviewed handoff is Chunk 11: retained fact, flow, and module
  types.

## Coverage

Reviewed the Chunk 10 types listed in `CODEBASE_STRUCTURE_CORE.md` across
occurrence indexes and keys, linked overlays and identity maps, constrained
argument preparation/evaluation, evidence accumulation, candidate iterators,
and event query views, with representative callers in fact construction and
project projection. Existing Chunk 1–9 findings were checked to avoid
re-reporting fact-table pairing, summary raw/effective arguments, project
identity overlays, and generic bounded worklist protocols. READ-051 through
READ-055 are marked applied above.
