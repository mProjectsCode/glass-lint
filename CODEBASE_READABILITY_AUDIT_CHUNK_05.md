# Codebase Readability Audit

## Summary

Chunk 5 owns the local artifact/cache boundary and the lowering transition
from parsed syntax to immutable semantic state. The cache correctly separates
source-independent semantic data from path-local reporting context, but the
lowering APIs still blur successful emptiness with disabled derived phases,
charge one shared budget at multiple abstraction levels, and use project
status to carry file-local failures until a later rewrite. Cache hits also
make an avoidable transient-artifact round trip. These are ownership and
phase-contract problems rather than storage or formatting concerns.

## Findings

### Derived-phase availability and incomplete outputs

#### [x] READ-017 — Preserve disabled derived phases as incomplete outcomes

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/mod.rs:13-59`; `glass-lint-core/src/analysis/facts/mod.rs:578-615`; `glass-lint-core/src/analysis/flow/effect/mod.rs:480-528, 655-667`; `glass-lint-core/src/analysis/local.rs:386-398`

`DerivedPhaseAvailability::DisabledByIncompleteAnalysis` is passed into fact
index and effect construction, but the disabled paths return ordinary empty
values: `SemanticFacts` retains an empty `OccurrenceIndexes`, and
`FunctionEffectsBuilder::finish` returns `FunctionEffects::default()`, whose
completion is the default complete state. Consumers can therefore observe an
empty index/effect collection without a phase-owned distinction between “the
source has no matches/effects” and “the phase was deliberately not computed.”
The artifact status is stored separately, so every caller must remember to
consult both unrelated APIs to interpret an empty result safely.

**Recommendation:** Keep the existing artifact-level
`DerivedPhaseCapabilities` as the policy owner, but attach the selected
availability/completeness to each derived value that can otherwise look like a
valid empty result. Do not turn this normal fail-closed state into a `Result`:
empty data is still useful for analyzed sources, while disabled data must be
queryably unavailable. Preserve deterministic empty results for genuinely
analyzed sources and keep incomplete phases from establishing findings.

**Fix Applied:** Attached `DerivedPhaseAvailability` to occurrence indexes
and function effects so disabled derived phases remain empty and fail closed
without being indistinguishable from genuinely analyzed empty results.
Production matcher and flow projection now consult the availability state,
and focused lowering tests cover both disabled and complete derivations.

### Semantic budget ownership

#### [x] READ-018 — Charge each semantic operation at one owner

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/mod.rs:379-411`; `glass-lint-core/src/analysis/resolution/mod.rs:301-306`; representative callers `glass-lint-core/src/analysis/facts/arguments.rs:152-157` and `glass-lint-core/src/analysis/facts/calls/mod.rs:92-99`

`FactBuilder::intern_name` charges the shared `SemanticBudget` and then calls
`Resolver::intern_name`, which charges the same budget again; property paths
also charge in `append_path` before reaching the name helper. The budget
therefore counts a single fact-level name operation according to the call
stack rather than one stable semantic operation, and changing a helper’s
implementation can silently change exhaustion behavior. Most other callers
also ignore the boolean returned by `try_charge`, leaving admission and
failure interpretation split between the caller and the budget owner.

**Recommendation:** Give one domain owner responsibility for charging each
operation, preferably `Resolver` for name interning and a path owner for path
allocation, then remove wrapper-side precharges and make the admission result
explicit at the boundary that can stop or mark the operation incomplete. Keep
the shared total semantic limit and sticky fail-closed exhaustion behavior, but
make repeated names, property-path names, and direct resolver calls consume a
documented and consistent unit of budget.

**Fix Applied:** Made `Resolver::intern_name` the sole semantic-budget owner
for fact-level name interning. The fact builder no longer precharges the same
operation, and exhausted budget/name-table admission is explicit and fail
closed. Updated the semantic-budget transition test for the new accounting.
Verified with `make fmt && make ci`.

### Local status and phase boundaries

#### [ ] READ-019 — Give local lowering failures an artifact/file scope

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:244-277`; `glass-lint-core/src/analysis/lowering/status.rs:71-130`; `glass-lint-core/src/analysis/project/linker/mod.rs:90-100`

Lowering records scope, fact, path, name, and value failures as
`StatusScope::Project`, even though each lowering invocation analyzes one
source file. The linker later calls `AnalysisStatus::for_local_file` to rewrite
every project-scoped entry into a file-scoped entry, so the same enum value has
different meaning before and after linking and true project-scoped statuses
cannot be distinguished from provisional local ones. This makes status
ownership depend on a convention between lowering and linking rather than on
the type that records the failure.

**Recommendation:** Add an explicit local-artifact/file scope or a separate
local lowering status that is materialized with the source path before it is
merged into project status; reserve `StatusScope::Project` for linking and
project-wide phases. Replace the broad `for_local_file` rewrite with a typed
conversion that can only promote local status, preserving deterministic
diagnostic ordering and the existing file/project report split. Keep parse
failures and genuinely project-wide link/flow failures on their current owners.

**Fix Applied:** None so far.

### Artifact cache and attachment lifecycle

#### [ ] READ-020 — Attach cache hits directly to the local artifact boundary

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/local.rs:226-246, 292-308, 421-425`; `glass-lint-core/src/project/session/mod.rs:134-145`; `glass-lint-core/src/project/session/artifacts.rs:130-145`

The cache stores a `SharedSemanticArtifact` without the original path, then
`ArtifactCacheHandle::get_lowered` reconstructs a `LoweredSource` with the
current path and line index. A cache hit immediately passes that value to
`AnalysisArtifacts::record_lowered`, which consumes it only to construct a
`LocalArtifact`; the semantic `Arc` and source attachment are rebuilt through
two adjacent wrapper transitions. The lifecycle distinction is valid, but the
cache-hit path duplicates the attachment conversion and exposes a transient
lowering type after lowering has already been completed.

**Recommendation:** Add a cache API that reattaches the current
`LocatedSourceContext` directly into a `LocalArtifact`, or give the artifact
manager one shared attachment constructor used by both cache-hit and fresh
lowering paths. Keep `SharedSemanticArtifact` path-independent, derive the
current path and line index from the requested `SourceFile`, and preserve the
fresh-lowering `LoweredSource` boundary for parse/lowering errors and observer
events. Delete the cache-hit-only `LoweredSource` reconstruction once callers
consume the same local-artifact contract.

**Fix Applied:** None so far.

## Systemic Themes

- Lowering produces several parallel status, capability, and derived-data
  channels. A typed phase outcome should keep availability and incompleteness
  attached to the data that consumers actually query.
- Shared budgets and status scopes are currently interpreted by callers at
  different abstraction levels. Their owners should define operation units
  and lifecycle scopes once, rather than relying on precharge and rewrite
  conventions.
- The cache’s reusable semantic state and path-local attachment are the right
  separation; the remaining simplification is to centralize reattachment,
  not to collapse cache and project lifetimes.

## Decisions

- Use status-bearing derived values, not `Result`, for disabled indexes and
  effects. Disabled-by-incomplete-analysis is an expected bounded outcome, not
  an API failure; the value must still distinguish it from an analyzed empty
  collection. Keep the capability decision at the artifact boundary and make
  the derived owners expose the distinction directly.
- Charge a semantic operation when its owning domain performs the bounded
  action: resolver name interning once per interning request, path storage once
  per path append, and fact emission once per fact admission. A lookup that
  reuses an existing name is not a second insertion charge. Remove wrapper
  precharges and make the owner’s failed admission visible to the caller that
  must mark the result incomplete.
- Keep lowering path-independent so cached semantic artifacts remain reusable.
  Convert local status to `StatusScope::File` exactly once when attaching the
  artifact to its current `LocalArtifact`; reserve project scope for genuine
  project/linking status and remove the broad rewrite convention.

## Coverage

Reviewed only Chunk 5, “Local artifacts and lowering,” from
`CODEBASE_STRUCTURE_CORE.md`, including local semantic artifacts, cache keys
and handles, source-context attachment, project modules, lowering transitions,
span normalization, semantic budgets, completion capabilities, and analysis
status/diagnostic types. Existing Chunk 1 through Chunk 4 audit history was
used to continue IDs at READ-017. No source, test, configuration, dependency,
or other documentation files were changed; this chunk audit file is the only
new artifact.
