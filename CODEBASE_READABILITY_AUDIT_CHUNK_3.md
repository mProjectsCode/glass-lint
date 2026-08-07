# Codebase Readability Audit — Chunk 3

## Summary

Chunk 3 owns local artifact caching and lowering, occurrence-index projection,
query resolution, constrained argument matching, and evidence normalization.
The boundaries are generally strong: facts are lowered once, indexes are
provider-neutral, occurrence buckets are normalized before querying, and
linked overlays preserve deterministic local-versus-project identity behavior.
The main maintainability risks are split lifecycle decisions and duplicated
representations at the cache and matcher boundaries, plus a few long
normalization/orchestration functions that combine policy phases.

## Findings

### Cache identity ownership

#### [x] READ-010 — Make cache identity own its fingerprint representation

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/local.rs:46-185,243-354`

`ArtifactCacheKey` stores the complete source, language, normalization mode,
environment, local limits, engine version, and a separately computed
`ArtifactFingerprint`. The fingerprint encoder independently repeats the
cache-affecting dimensions, while both `ArtifactCache::get` and
`ArtifactCache::insert` compare the fingerprint and then the full key. This
is deliberate collision protection, but the invariant that both
representations describe exactly the same artifact inputs is maintained by
convention: adding a lowering input requires updating the key fields, the
fingerprint encoding, and the versioning policy together.

**Recommendation:** Introduce one private cache-identity value object whose
constructor owns the canonical input encoding and derives the collision-check
fingerprint from that representation. Keep the full inputs available for
collision verification, but make cache lookup/insertion accept the identity
object rather than separately passing a fingerprint alongside a key. Delete
the duplicated fingerprint plumbing in `ArtifactCacheHandle` and the repeated
`entry.fingerprint == fp && entry.key == ...` checks once the owner is
established. Preserve full-key collision verification, source-independent
semantic reuse, engine-version invalidation, and exclusion of downstream rule,
evidence, link, and flow limits from local identity.

**Fix Applied:** Cache lookup/insertion now pass only `ArtifactCacheKey`; the
cache entry no longer stores or receives a parallel fingerprint. The key’s
canonical constructor remains the sole fingerprint derivation point, and
lookup still verifies both the fingerprint and complete key inputs.

### Lowering completion policy

#### [ ] READ-011 — Let one completion policy produce status and capabilities

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:186-287`

`LoweringCompletion::assess` computes `budget_exhausted` directly from the
semantic budget, fact stream, path table, value arena, and structural-validity
flags, then separately calls `check_facts_budget`,
`check_invalid_parser_span`, and `check_name_exhaustion` to populate
`AnalysisStatus`. The resulting `status` and `LoweringCapabilities` are
parallel views of one completion decision, but they are not derived from one
typed outcome: a new bounded resource or a new capability can be added to one
side and omitted from the other. `ResolvedProgram::freeze` then consumes the
capability booleans while callers consume the status diagnostics.

**Recommendation:** Give a lowering-completion policy object ownership of the
resource checks and have it return a typed outcome containing status plus
capabilities (or capability-specific proofs). Make the individual budget and
parser/name checks feed that outcome instead of maintaining a second boolean
expression in `assess`. Delete the duplicated completion-condition list and
make export-origin derivation and effects enablement consume the same outcome.
Preserve the distinction between an incomplete artifact and a usable
possible witness, all existing diagnostic reasons, independent name-table
exhaustion reporting, and fail-closed capability decisions for exhausted
facts, paths, values, or invalid parser spans.

**Fix Applied:** None so far.

### Artifact sealing orchestration

#### [x] READ-012 — Split `ResolvedProgram::freeze` into named sealing phases

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:324-373`

The consuming `ResolvedProgram::freeze` method currently coordinates four
different responsibilities: it assesses completion, conditionally traverses
the module interface and resolver to derive export origins, mutates the
stream for late name exhaustion, and freezes/builds the final
`SemanticArtifact`. Each step is individually meaningful, but the method’s
destructuring and phase-ordering details make the artifact lifecycle hard to
read and force future lowering outputs to be threaded through one growing
function. The late `stream.mark_name_exhausted()` mutation is especially easy
to overlook because it is separated from `LoweringCompletion::assess`.

**Recommendation:** Keep the consuming transition, but delegate to named
operations for completion assessment, retained export-origin derivation,
final stream annotations, and immutable artifact assembly. The owner of each
phase should accept and return the narrow state it changes, with a final
`seal` operation making the one-way transition explicit. Delete the mixed
branching and repeated destructuring from `freeze` after migration. Preserve
the order in which status is assessed, name exhaustion is recorded, resolver
tables are frozen, export origins are suppressed when incomplete, and effects
capabilities are passed into `SemanticArtifact`.

**Fix Applied:** Split the consuming transition into named completion
assessment, export-origin derivation, name-exhaustion annotation, and final
stream sealing phases. The existing status ordering, capability gates, and
resolver freeze transition are preserved.

### Matcher project context

#### [ ] READ-013 — Encapsulate the shared matcher/project evaluation context

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/matching/arguments/mod.rs:43-140`; `glass-lint-core/src/analysis/project/projection.rs:491-526`

Projection constructs `MatcherArtifact::from_facts` with module identities,
then separately constructs `MatcherProjectOverlay::from_identities` with the
same module identities plus call-result identities. The first type owns the
linked occurrence overlay and frozen fact/index references; the second owns
identity maps used by constrained argument evaluation. `project_facts` must
pass both values through `ProjectionInputs`, so the relationship between
these two views of one module’s project context is maintained by the caller
rather than by a single matcher boundary. The separate test constructors
(`from_parts`, `from_parts_with_overlay`, and `MatcherProjectOverlay::new`)
also expose the split lifecycle directly.

**Recommendation:** Introduce one private matcher evaluation context built
from the immutable facts and the two optional identity maps. Let it construct
the occurrence overlay and retain the argument-resolution identities, then
pass that context to constrained matching. Remove the parallel production
constructors and test-only arguments that do not participate in the same
lifecycle once callers migrate. Preserve the distinction between occurrence
remapping identities and call-result identities, optional work based on the
compiled plan, artifact-local borrowing, bounded overlay operation counts,
and ambiguity/unknown fail-closed behavior.

**Fix Applied:** None so far.

### Evidence normalization phases

#### [x] READ-014 — Separate evidence grouping from bounded presentation

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/matching/evidence.rs:69-163`

`normalize_evidence` drains raw evidence, groups counts and certainty, filters
empty spans, sorts and deduplicates trace occurrences, applies the per-group
occurrence limit, constructs report items, sorts groups by first span, and
applies the global group limit. These are two related but distinct policies:
semantic aggregation and bounded deterministic presentation. Keeping them in
one in-place function hides which fields are retained from discarded
occurrences and makes a future change to one limit or ordering rule likely to
touch all phases at once.

**Recommendation:** Add a private evidence accumulator that owns key merging,
count/certainty aggregation, and occurrence identity deduplication; then pass
its completed groups through a separate bounded presenter that applies
per-group and global limits and deterministic ordering. Replace the
`drain`/rebuild choreography with those named operations and delete the
phase-mixed branches from `normalize_evidence`. Preserve exact original
counts, possible-certainty propagation, empty-span filtering, distinct traces
at the same span, per-group truncation markers, first-span/kind/symbol order,
and global truncation markers.

**Fix Applied:** Added separate `EvidenceAccumulator` and
`EvidencePresenter` owners. Accumulation now handles key merging, certainty,
empty-span filtering, and trace identity deduplication; presentation handles
per-group/global bounds and deterministic ordering without changing counts or
truncation behavior.

## Systemic Themes

- The chunk has good provider-neutral ownership of facts, indexes, and query
  semantics, but lifecycle decisions still cross status, capability, and
  artifact-assembly representations.
- Bounded behavior is explicit and deterministic; refactors must keep full
  cache collision checks, occurrence normalization, evidence limits, and
  overlay operation accounting intact.
- Matcher boundaries should keep local facts, linked occurrence remapping,
  and call-result identity resolution distinct semantically while presenting
  one coherent evaluation context to callers.

## Decisions

- `ArtifactFingerprint` is only a deterministic collision prefilter. The
  cache identity must retain full-key equality after a fingerprint match; a
  hash match alone is never a cache hit.
- Current lowering capabilities have one completeness prerequisite and no
  independent enablement contract. Keep the bundled completion decision for
  now; introduce capability-specific proofs only when a capability has a
  distinct producer and consumer, rather than anticipating that split.

## Coverage

Reviewed all modules listed in Chunk 3 of `CODEBASE_STRUCTURE_CORE.md`:

- `analysis::local`, `analysis::lowering`, `analysis::lowering::budget`,
  `analysis::lowering::status`, `analysis::matching`,
  `analysis::matching::arguments`, `analysis::matching::arguments::evaluator`,
  `analysis::matching::arguments::identity`, `analysis::matching::build`,
  `analysis::matching::evidence`, `analysis::matching::identity_map`,
  `analysis::matching::indexes`, `analysis::matching::occurrence`,
  `analysis::matching::query`, and `analysis::matching::query::view`.

Representative callers in project projection, session artifact caching, local
lowering, and matching unit tests were checked for cache identity, lifecycle,
overlay ownership, bounded fallback evaluation, and deterministic evidence
behavior.
