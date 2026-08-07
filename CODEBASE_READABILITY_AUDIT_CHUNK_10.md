# Codebase Readability Audit — Chunk 10

## Summary

Chunk 10 owns the provider-neutral occurrence indexes and the query-side
matching views that consume them. The design correctly builds indexes once
from retained facts, keeps module identity remapping in a linked overlay, and
defers final evidence normalization to the project boundary. The main
readability risks are that candidate iterators expose different ordering and
ownership contracts behind one enum, the event view stores duplicate physical
references to support generic helpers, and project-identity precedence is
reimplemented in the constrained matcher.

The shared matcher/project context and evidence-normalization concerns were
reviewed against the existing Chunk 3 report and are not repeated here.

## Findings

### Candidate occurrence contract

#### [ ] READ-045 — Give candidate occurrence selection one explicit contract

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/matching/occurrence.rs:17-51,53-225,228-328,390-405`; callers in `matching/query/mod.rs:53-151`

`CandidateOccurrences` presents indexed, merged, package-scanned, and
predicate-scanned results as one iterator type, but the variants do not share
the same semantic contract. `Indexed` yields a normalized bucket; the merged
iterators preserve duplicate occurrences across buckets; package iteration
emits base keys and then overlay keys rather than one global occurrence order;
and `OccurrenceIndex::matching` allocates a key-order `Vec` without a final
sort or cross-bucket deduplication. The project-level
`normalize_evidence` pass currently repairs these differences, so callers of
the internal selection API must know that “candidate occurrences” are only
presentation-ready after a later boundary.

This makes ordering, duplicate counting, and allocation behavior depend on
which identity path happened to select the candidates. A new consumer such as
profiling, early truncation, or a direct matcher can silently observe a
different result from an exact lookup, and a future query can accidentally
rely on the final report normalizer for a correctness invariant it does not
own.

**Recommendation:** Split the raw lazy selection from the normalized evidence
selection, or make a private `OccurrenceSelection` owner expose one explicit
operation that preserves duplicate physical events for counting and performs
span/fact/trace deduplication only at final evidence grouping. Keep exact
lookups and linked package scans lazy where possible, but make the boundary at
which sorting, deduplication, and count preservation occur explicit instead of
encoding four contracts in one iterator enum. Preserve module-overlay masking,
deterministic evidence order, and the distinction between duplicate physical
occurrences and distinct semantic facts.

**Fix Applied:** None so far.

### Event index view

#### [x] READ-046 — Remove duplicate physical references from `EventIndexView`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/query/mod.rs:160-206`; `matching/query/view.rs:26-72,102-216,252-306`

`EventIndexView` uses generic helper methods such as `resolve_any`,
`resolve_rooted`, `member_path`, and `global_index` to serve several event
families. As a result, `build_event_view` places the same physical index in
two semantically different fields for some variants: `PropertyWrite` stores
`rooted_writes` as both `paths` and `rooted`, while `Construct` stores
`global_constructors` as both `strings` and `global`. The enum shape therefore
requires callers to preserve an aliasing invariant that is not represented by
the type, and the generic helpers make it possible for the two fields to drift
when a new construction or write index is introduced.

**Recommendation:** Collapse each duplicate pair into one event-specific
field, or introduce narrow event lookup descriptors whose operations expose
only the identities valid for that event. Delete the alias-only fields and
the helper branches that exist solely to accommodate them, while preserving
the current rejection of unsupported identity/event combinations, rooted
global-object matching, constructor-name fallback, module overlays, and
property-write behavior.

**Fix Applied:** Collapsed duplicate `PropertyWrite` storage into one
`writes` index and duplicate `Construct` storage into one `global` index;
the view’s operation-specific patterns now borrow the same named field
without alias-only members. Verified with `make fmt && make ci`.

### Effective project identity

#### [ ] READ-047 — Centralize constrained-matcher identity precedence

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:106-171`; `matching/arguments/mod.rs:105-132`; construction in `analysis/project/projection.rs:491-522`

`EffectiveIdentityResolver::static_string` and
`EffectiveIdentityResolver::call_provenance` independently implement the
same overlay precedence: call-result identity first, module identity from
the argument provenance second, then a local fallback. They differ only in
how the selected resolution is converted and what fallback is available, so
the lookup policy is duplicated inside two methods. `MatcherProjectOverlay`
also exposes the two raw optional maps directly, leaving the constrained
matcher boundary responsible for remembering which map wins and how
artifact-local `ValueId` lookup relates to module provenance.

Adding another identity-consuming matcher operation now requires copying the
precedence sequence and deciding its own fallback. That can make a call
provenance match use a different project identity from a static-string or
object match without any type-level indication that the policies diverged.

**Recommendation:** Give the project overlay or `EffectiveIdentityResolver`
one lookup operation that returns the effective `ExportResolution` for a
`ValueId` plus provenance, then keep conversion to static strings or call
provenance at the narrow consumers. Hide the raw result-identity map behind
that domain operation and delete the repeated `result_identity(...).or_else`
and `module_identity(...).or_else` chains. Preserve result-identity
precedence, module-identity fallback, local value fallback, raw-provenance
fallback, artifact-local IDs, and fail-closed behavior for ambiguous,
unknown, or unsupported resolutions.

**Fix Applied:** None so far.

## Systemic Themes

- Matching has a deliberate two-stage contract: shared fact indexes provide
  candidates, and project presentation normalizes bounded evidence. That
  boundary should be explicit in the candidate API rather than inferred from
  the final report path.
- Event-specific semantic restrictions are currently implemented through a
  broad enum and generic accessors. Narrower views can reduce borrow plumbing
  without collapsing distinct call, member, construction, literal, and
  rooted identity semantics.
- Project identities remain provider-neutral and must preserve strict
  artifact-local identity, deterministic output, and fail-closed unknown or
  ambiguous alternatives. Refactors must not turn a linked overlay into a
  textual or cross-artifact name match.

## Decisions

- Raw candidate iteration preserves duplicate physical occurrences until the
  final evidence accumulator. Counts represent observed events, while the
  final boundary deduplicates equivalent span/fact/trace occurrences for
  presentation.
- Duplicate `EventIndexView` fields are compatibility remnants, not a planned
  shared interface. Remove the alias machinery in one migration and keep
  event-specific lookup descriptors as the narrow internal API.
- Effective identity uses a typed result distinguishing absent local data,
  explicit unknown, and ambiguity. `Option` remains appropriate only for a
  lookup that truly means “no local entry”; it must not collapse unresolved
  project identity into absence.

## Coverage

Reviewed all types listed in Chunk 10 of `CODEBASE_STRUCTURE_CORE.md`:

- Occurrence storage and overlays: `BorrowedGlobalBuckets`,
  `BorrowedModuleBuckets`, `LinkedOccurrenceView`,
  `ModuleOccurrenceOverlay`, `ModuleOverlayKind`, and `OccurrenceIndexes`.
- Argument matching: `ConstrainedRoot`, `MatcherArtifact`,
  `MatcherEvaluationContext`, `MatcherProjectOverlay`,
  `PreparedConstrainedRoot`, `PreparedClausePaths`,
  `EffectiveIdentityResolver`, `EvaluationOperations`, and
  `MatcherEvaluator`.
- Evidence and identity: `EvidenceAccum`, `EvidenceKey`, and
  `ModuleIdentityMap`.
- Indexes and occurrences: `CallIndexes`, `ConstructionIndexes`,
  `LiteralIndexes`, `MemberIndexes`, `BorrowedOccurrenceIter`,
  `BorrowedPackageOccurrenceIter`, `CandidateOccurrences`, `InstanceMemberKey`,
  `MergeItem`, `MergeState`, `ModuleExportKey`, `ModuleOccurrences`,
  `NameOccurrences`, `Occurrence`, `OccurrenceIndex`, `Occurrences`,
  `PackageKeyPredicate`, `PackageMatchKind`, `PackageOverlay`,
  `ReturnedMemberKey`, and `EventIndexView`.

No source, test, configuration, or existing audit files were changed. The
Chunk 3 findings for shared matcher/project context and evidence grouping were
cross-checked and intentionally not duplicated.
