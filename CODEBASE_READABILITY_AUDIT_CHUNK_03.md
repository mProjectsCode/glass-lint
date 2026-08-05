# Codebase Readability Audit — Chunk 3

## Summary

Chunk 3 covers the immutable local artifact boundary, parser-to-semantic
lowering, bounded lowering status, occurrence indexes, linked occurrence
overlays, and local matcher execution. The implementation correctly keeps AST
details private, builds matcher-independent indexes once, and preserves
fail-closed behavior for incomplete artifacts. The main readability and API
risks are lifecycle protocols that are still assembled by neighboring callers:
cache entries are reconstructed into phase objects, lowering completion is
computed in parallel with derived-phase policy, and matching views receive
several independently supplied selectors and identity stores.

The highest-value improvements are to give cache conversion and lowering
completion a single owner, make linked overlay storage and event selection
type-directed, and bundle the matcher context and identity precedence. These
changes should reduce coordination code without collapsing local artifact
identity, project overlays, or uncertainty states.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Local artifact and lowering lifecycle

#### [ ] READ-014 — Encapsulate cache-to-lowered-source conversion

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture / ownership
- **Location:** `glass-lint-core/src/analysis/local.rs:220-241, 264-337`,
  `analysis/lowering/mod.rs:97-116`,
  `project/session/artifacts.rs:87-103, 154-180`

The lowered-source lifecycle is split among `LoweredSource`,
`SharedSemanticArtifact`, `LocalArtifact`, and the session artifact helper.
`AnalysisArtifacts::record_lowered` decomposes a `LoweredSource` with
`into_parts` and rebuilds a `LocalArtifact`; a cache hit separately combines
the current source path with `clone_source_index` and `clone_semantic` to
reconstruct another `LoweredSource`. `ArtifactCacheHandle` exposes only raw
`get` and `insert`, so callers must know which parts are path-specific and
which parts are safe to share.

The invariant is that cached semantic data contains no path-specific context,
while every returned local artifact gets the current source path and the
cached line index for the identical source key. That protocol is correct but
storage-shaped: a future caller can pair a semantic artifact with the wrong
source context or cache an unsuccessful/partial result without a type-level
signal. The conversion logic also makes the phase boundary harder to read and
duplicates cache insertion/retrieval mechanics outside the cache owner.

Give the cache owner narrow operations such as a cache-hit conversion from a
`SourceFile` and insertion from a `LoweredSource`, or introduce a private
cache-entry conversion type that owns these transitions. Delete the public
`clone_semantic`/`clone_source_index` plumbing and the session-side
reconstruction once callers use the semantic operations. Preserve full-key
collision verification, path-local source attachment, source-map reuse,
successful-artifact-only caching, FIFO eviction, and the distinction between
parse failure and an incomplete semantic artifact.

**Fix Applied:** None so far.

#### [x] READ-015 — Make lowering completion and derived-phase policy one result

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture / Complexity / state protocol
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:193-239,
  278-349`

`ResolvedProgram::freeze` mixes status recording, budget classification,
export-origin derivation, name-exhaustion marking, resolver freezing, fact
construction, and final artifact construction. It also computes a broad
`budget_exhausted` boolean at lines 303-307 while `check_facts_budget`
independently classifies overlapping stream, path, value, and semantic-budget
conditions. The boolean controls whether export origins and effects are
derived, whereas the status helpers control diagnostics, so one new
incompleteness condition must be updated in two policy paths.

The leaked invariant is the relationship between a lowering completion reason
and which derived data is safe to expose. A new budget or invalid state can be
reported but still permit an unsafe derived phase, or skip a phase without a
matching diagnostic, unless both lists remain synchronized. This is a mixed
responsibility function rather than a problem with the existing conservative
policy.

Introduce a private lowering-completion result that records the reason set and
explicit capabilities such as whether export provenance and effects may be
derived. Let `freeze` consume that result in named phases: collect status,
apply stream markers, derive permitted exports, and assemble the immutable
artifact. Delete the duplicate condition list and retain partial facts for
diagnostics, name-table exhaustion behavior, invalid parser-span handling,
fail-closed indexes/effects, and deterministic status ordering.

**Fix Applied:** Added a private `LoweringCompletion` result containing the
canonical status entries and explicit export-origin/effect capabilities. Its
single assessment owns the scope, budget, parser-span, and name-exhaustion
policy; `ResolvedProgram::freeze` now consumes those capabilities instead of
recomputing a parallel exhaustion boolean.

**Verification:** `make fmt && make ci` passes, including 780 core tests,
lowering lifecycle tests, workspace checks, end-to-end/provider harnesses,
doctests, generated-rule validation, and examples.

### Matching indexes and linked overlays

#### [ ] READ-016 — Give linked occurrence buckets one owning dispatch type

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture / Newtype / Duplication
- **Location:** `glass-lint-core/src/analysis/matching/mod.rs:39-62,
  64-233`

`LinkedOccurrenceView` stores five parallel maps with identical bucket
representations (`module_calls`, `member_calls`, `member_reads`,
`module_classes`, and `module_constructors`) plus a shared masking set.
`ModuleOverlayKind` then dispatches over those maps separately in
`build`, `module_buckets_mut`, `module_buckets`, and the query view's
`module_view`. Adding a new linked occurrence category requires editing all of
these lists, and the relationship between a remapped bucket and its mask is
maintained by callers rather than by a bucket owner.

The semantic distinctions between call, member, read, class, and constructor
overlays are real, but their storage and lifecycle are the same. The current
parallel representation makes remapping, package lookup, and fallback masking
easy to update inconsistently and makes `remap` carry both identity policy and
collection dispatch.

Introduce a private `ModuleOccurrenceOverlay`/bucket collection that owns
kind-to-bucket access, remapping, masking, and package-overlay construction.
Keep `ModuleOverlayKind` as the typed vocabulary, but delete the repeated
mutable/immutable match arms from `LinkedOccurrenceView` and the query layer.
Preserve deterministic bucket order, exact-key masking, wildcard identity
fallback, external remapping, global promotion only for ordinary calls, and
the rule that ambiguous or unknown identities do not create occurrences.

**Fix Applied:** None so far.

#### [ ] READ-017 — Bind event selection to the occurrence query view

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture / Complexity
- **Location:** `glass-lint-core/src/analysis/matching/query/mod.rs:36-106,
  165-209`, `matching/query/view.rs:27-103, 105-232, 267-316`

`OccurrenceIndexes::occurrences_for_indexed` first builds an
`EventIndexView` from an `EventPredicate`, then passes both the resulting view
and the same event back into `EventIndexView::resolve`. The view therefore
accepts a selector that is already represented by its enum variant. Several
resolution methods re-check the pairing with guards such as
`matches!(event, ...)`, while `module_view` independently maps the variant to
an overlay kind. The caller must preserve this two-part contract even though
the query is one compiled identity/event clause.

This is a bad internal API boundary and a hard-to-read dispatch path, not a
claim that current callers pass mismatched events. The duplicated event
argument and variant guards make it possible for a new event kind to select
the wrong index or silently return `None`, and they spread the event-to-index
mapping across two modules.

Make `OccurrenceIndexes` own a clause-resolution operation that constructs a
view whose event-specific selector is inseparable from the view, or make
`EventIndexView` retain the prepared event data and accept only identity,
names, and overlay inputs. Delete the repeated event parameter and
variant/event compatibility matches after migration. Preserve rooted versus
heuristic member semantics, literal and package matching, module overlay
masking, global-object aliases, and fail-closed `None` results for unsupported
identity/event combinations.

**Fix Applied:** None so far.

### Constrained matcher execution

#### [ ] READ-018 — Bundle the local artifact and project overlay context

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture / Newtype
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:28-102`,
  `analysis/matching/arguments/evaluator.rs:100-120`,
  `analysis/project/projection.rs:171-215, 457-493`

`compute_constrained_evidence_from_stream_with_overlay` accepts seven
independent inputs: frozen facts, occurrence indexes, physical roots,
evidence storage, linked occurrence overlay, module identities, and call
result identities. It immediately reconstructs a second context and then
extracts names and values from the stream before constructing
`MatcherEvaluator`. The projection caller assembles the same relationship
manually from `module.local().facts()`, identities, result identities, and an
optional overlay.

The leaked invariant is that the stream, names, values, occurrence indexes,
and overlay all belong to the same local module, while each optional identity
map is the corresponding project projection for that module. The borrow
checker enforces lifetimes but not semantic pairing; this API permits a
caller to combine data from different artifacts or to provide result
identities without the module identity context that produced them. It also
makes the constrained matcher entry point harder to understand than the
actual query operation.

Create a private matcher input/context owned by the local semantic artifact,
with a separate typed project-overlay attachment for linked identities and
call-result identities. Have the projection boundary construct that context
once and pass it to constrained evaluation; remove the parallel raw arguments
and the immediate context reconstruction. Preserve matcher-independent fact
construction, local/project identity separation, static value/name table
coherence, overlay masking, bounded fallback scans, deterministic operation
counts, and unknown/ambiguous identity rejection.

**Fix Applied:** None so far.

#### [ ] READ-019 — Centralize effective identity overlay precedence

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / API / Architecture
- **Location:** `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:100-120,
  183-238`

`MatcherEvaluator::argument_with_overlay` and
`overlaid_call_provenance` independently apply project result identities,
module identities, and local fallback data. Both use `lookup_identity`, but
the precedence and conversion logic is repeated in separate paths: argument
views layer static strings over value-table information, while call matching
layers effective call provenance over the raw provenance. These are two
consumers of the same “effective identity for this local value under this
project overlay” concept.

The maintenance risk is semantic drift in precedence. A future identity kind
or an ambiguity rule can be added to one path but not the other, causing a
call identity and its argument identity to disagree about the same qualified
value. The distinction between result identity by `ValueId`, module identity
by provenance, and local static-value fallback is valid; the conversion
policy is what is duplicated.

Give a private evaluator-owned identity resolver or prepared value view the
layered lookup operations, and have both call and argument matching consume
that result. Delete the repeated result/module/fallback branches while
retaining the current precedence, rejection of ambiguous/unknown resolutions,
rooted-member and static-object extraction, and the fact that a result
identity must remain tied to its local `ValueId`.

**Fix Applied:** None so far.

### Evidence boundary

#### [ ] READ-020 — Make evidence normalization a single pipeline boundary

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API / Duplication / Complexity
- **Location:** `glass-lint-core/src/analysis/matching/query/mod.rs:36-95`,
  `matching/mod.rs:322-355`, `matching/evidence.rs:75-152`,
  `analysis/project/projection.rs:254-269, 537-558`

Occurrence query assembly converts candidates into `ClassificationEvidence`,
sorts those groups in `OccurrenceIndexes::evidence_for_with_overlay`, and
then `ProjectMatcherModel::evidence_for` merges projected flow evidence and
runs `normalize_evidence` again. The latter owns grouping, count aggregation,
occurrence sorting/deduplication, per-group truncation, and global group
limits, while the former still owns a partial ordering policy. `RuleEvidenceTable`
also stores grouped evidence before that final normalization.

The invariant that evidence is deterministic, grouped by `(kind, symbol)`,
bounded, and trace-preserving is therefore split across the index query,
flow table, and report-facing projection. The current extra sort is benign,
but the boundary makes it unclear whether a new caller may consume raw
evidence or must normalize it, and a future ordering change can be applied in
only one path.

Choose one private evidence accumulator/normalization boundary for both
indexed and projected evidence. Make index and flow producers append raw
groups or use the same accumulator, then delete the redundant pre-sort or
other duplicate grouping step. Preserve total event counts, certainty
combination, trace identity in deduplication, per-group and global limits,
and deterministic ordering of the final report.

**Fix Applied:** None so far.

## Systemic Themes

Chunk 3 repeatedly passes semantically related pieces as independent values:
semantic artifact plus source context, completion reason plus derived-phase
permission, overlay kind plus one of several parallel maps, event selector
plus event view, and local fact storage plus project identity overlays. These
are all internal APIs, but they sit at important architecture boundaries and
force callers to preserve invariants that their types do not express.

The existing typed occurrence keys, frozen fact stream, bounded candidate
iterators, and `AnalysisStatus` are good foundations. Refactors should extend
those owners and remove coordination code rather than introduce a second
semantic model or merge local artifacts with project-linked identity state.

Search signals used for this chunk included raw `into_parts`/clone-and-rebuild
conversions, duplicated exhaustion predicates, parallel maps selected by an
enum, methods accepting both a view and the selector that built it, long
optional-context signatures, repeated overlay precedence branches, and
multiple evidence sort/normalization passes.

## Open Questions

- Cache conversion should remain aware that source paths are not part of the
  reusable semantic cache entry; the cache owner may need a source argument to
  recreate only the path-local attachment.
- The lowering completion result should distinguish “facts retained for
  diagnostics” from “derived exports/effects safe to compute”; those are not
  the same as a simple complete/incomplete boolean.
- The next unreviewed handoff is Chunk 4: retained models, project linking,
  and resolution modules.

## Coverage

Reviewed every source file listed for Chunk 3 in `CODEBASE_STRUCTURE_CORE.md`:
`analysis/local.rs`; `analysis/lowering/mod.rs`, `budget.rs`, and `status.rs`;
and all `analysis/matching` modules including argument evaluator/identity,
index building, evidence, identity maps, occurrence storage, query views, and
their in-module tests. Representative callers in
`analysis/facts/mod.rs`, `analysis/project/projection.rs`, and
`project/session/artifacts.rs` were traced only to verify ownership and phase
contracts. No findings are marked applied.
