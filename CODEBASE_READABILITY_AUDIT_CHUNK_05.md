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

#### [ ] READ-017 — Preserve disabled derived phases as incomplete outcomes

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

**Recommendation:** Make each derived-phase owner return an explicit outcome
that carries availability/completeness alongside its data, or retain the
availability state inside `SemanticFacts` and `FunctionEffects` with narrow
queries such as `is_available`/`completion`. Do not use a successful default
empty collection for a disabled phase; preserve deterministic empty results
for genuinely analyzed sources and keep incomplete phases from establishing
findings or being mistaken for complete absence. Consolidate the capability
decision at the lowering/artifact boundary so fact indexes, export origins,
and effects use one typed policy.

**Fix Applied:** None so far.

### Semantic budget ownership

#### [ ] READ-018 — Charge each semantic operation at one owner

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

**Fix Applied:** None so far.

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

## Open Questions

- Should an unavailable derived phase be represented as a fallible
  `Result`/typed outcome or as an available collection carrying explicit
  completeness? The public matching contract must continue distinguishing
  unavailable work from a proven empty result.
- Which semantic operations are intended to consume the shared budget when a
  name is already present in the scope snapshot? The answer should be encoded
  by the budget-owning API and tested independently of call-stack shape.
- Should local lowering receive the source path directly so file-scoped status
  can be recorded at creation time, or should a local-status wrapper remain
  path-independent until project admission?

## Coverage

Reviewed only Chunk 5, “Local artifacts and lowering,” from
`CODEBASE_STRUCTURE_CORE.md`, including local semantic artifacts, cache keys
and handles, source-context attachment, project modules, lowering transitions,
span normalization, semantic budgets, completion capabilities, and analysis
status/diagnostic types. Existing Chunk 1 through Chunk 4 audit history was
used to continue IDs at READ-017. No source, test, configuration, dependency,
or other documentation files were changed; this chunk audit file is the only
new artifact.
