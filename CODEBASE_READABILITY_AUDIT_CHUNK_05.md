# Codebase Readability Audit — Chunk 5

This audit covers Chunk 5 of `CODEBASE_STRUCTURE_CORE.md`: local artifacts and
lowering. It is an architectural review only; no source changes were made.

## Summary

The local-artifact boundary has a strong shape: parsing and semantic lowering
happen once, cached semantic state is immutable and reusable, and path-specific
source context is reattached when a cache hit is consumed. The main readability
risks are in the policy surrounding that boundary. Derived-phase availability
is decided independently by completion capabilities, fact-stream validity, and
lazy effect state. Lowering completion has a builder and result type with the
same state, while the final freeze function coordinates several ownership
transitions in one place. Cache identity and status diagnostics also expose
parallel representations that make invariants less local and APIs more
positional than the surrounding architecture requires.

## Findings

### READ-001 — Derived-phase availability is represented by several independent gates

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture / API
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:234-315`; `glass-lint-core/src/analysis/local.rs:341-399`; `glass-lint-core/src/analysis/facts/mod.rs:505-529`; `glass-lint-core/src/analysis/flow/effect/mod.rs:522-552`
- **Representative callers:** `ResolvedProgram::freeze`, `SemanticFacts::from_lowering`, and `SemanticArtifact::effects`

The same question—whether derived semantic consumers may run on this local
artifact—is answered in several places. `LoweringCapabilities` records
`export_origins` and `effects`, `LoweringCompletionPolicy` disables those
booleans for selected fact failures, `SemanticFacts::build_index` independently
checks `FactStream::is_valid`, and `FunctionEffectsBuilder::new` independently
uses that same stream predicate. `SemanticArtifact` then stores another
`effects_enabled` boolean beside a lazy `OnceLock<FunctionEffects>` and returns
an empty effect set when the flag is false.

These gates are individually reasonable, but their ownership is distributed:
scope issues are recorded by the completion policy, index admission is owned
by the fact stream, effect admission is partly owned by the stream and partly
by the artifact, and export-origin admission is decided during freeze. A new
incomplete condition must be threaded through multiple representations, and a
future caller can accidentally treat an empty derived result as either “not
requested,” “disabled by incomplete analysis,” or “computed and empty.” That
ambiguity is especially important because incomplete paths must not establish
definite matches.

**Recommendation:** Define one private derived-phase availability value owned
by the lowering completion boundary, with explicit states for enabled,
disabled-by-incomplete-analysis, and not-requested/lazy where those
distinctions are required. Pass that value into index/effect construction or
store it in a typed artifact capability rather than reproducing boolean gates.
Keep lazy effect computation, bounded budgets, empty results for disabled
derived phases, and the fail-closed rule that incomplete facts cannot create a
definite witness.

**Fix Applied:** None so far.

### READ-002 — `ResolvedProgram::freeze` coordinates too many phase transitions

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:349-415`
- **Representative callers:** `Lowerer::lower_program` and `Lowerer::lower_source` ultimately consume the single `ResolvedProgram::freeze` transition

`ResolvedProgram::freeze` first assesses completion, derives export origins,
reads resolver exhaustion, destructures the resolved state, annotates the fact
stream, extracts the resolver-owned tables, freezes the stream, constructs
`SemanticFacts`, and finally constructs `SemanticArtifact`. This is the
canonical end of lowering, so the ordering is important, but the function
mixes completion policy, derived-data selection, phase-owned mutation, and
immutable artifact assembly in one sequence.

The mixed levels make it difficult to see which operations are policy and
which are mechanical transfer. For example, export origins depend on
capabilities before the resolver is consumed, while name exhaustion must be
marked before the stream is frozen. A future phase addition can be inserted in
the wrong place without a type-level indication of whether it needs the
building stream, the resolver, or the final artifact.

**Recommendation:** Keep one consuming lowering transition, but give it named
private phase values or methods for completion assessment, derived-data
collection, resolved-state sealing, and final artifact assembly. A typed
intermediate such as “sealed lowering inputs” can own the point after resolver
consumption instead of exposing a sequence of individual locals. Preserve
single traversal, resolver-before-stream sealing, capability-dependent export
origins/effects, deterministic output, and the current consuming lifecycle.

**Fix Applied:** None so far.

### READ-003 — Completion policy and completion result duplicate their entire state

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Conversion
- **Location:** `glass-lint-core/src/analysis/lowering/mod.rs:254-292`
- **Representative callers:** `LoweringCompletionPolicy::assess` builds the policy and `ResolvedProgram::assess_completion` immediately consumes it through `finish`

`LoweringCompletion` and `LoweringCompletionPolicy` have the same two fields:
`AnalysisStatus` and `LoweringCapabilities`. The policy mutates those fields
through `record_scope_issue` and `record_fact_failure`, and `finish` merely
moves them into the identically shaped result. This creates a parallel model
for a short-lived builder without adding a distinct invariant or ownership
boundary.

The duplication is easy to overlook when adding a capability or status detail:
the field must be added to both structs and the transfer must remain complete.
It also obscures whether the result differs semantically from the mutable
accumulator or is simply its finalized form.

**Recommendation:** Make one private completion type own both accumulation and
finalization, for example by giving `LoweringCompletion` private mutation
methods and consuming it directly, or by making the policy itself the
consuming result with a clearly named finalization operation. Remove the
duplicate field declaration and trivial `finish` transfer. Preserve the
ordered status recording, capability disabling, and immutable value passed to
artifact construction.

**Fix Applied:** None so far.

### READ-004 — Cache identity dimensions and match predicates are maintained in parallel

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Cache API
- **Location:** `glass-lint-core/src/analysis/local.rs:26-83,121-184,308-337`
- **Representative callers:** `ArtifactCacheHandle::get_lowered`, `ArtifactCacheHandle::insert_lowered`, and `ProjectCollection`'s `artifact_fingerprint`

The cache identity contract is spread across `LocalLoweringConfig`,
`ArtifactFingerprint::compute`, and `ArtifactCacheKey`'s full equality. The
same local-affecting limit dimensions are first stored in the config and then
manually serialized into the fingerprint. At the cache boundary, both
`ArtifactCache::get` and `ArtifactCache::insert` repeat the combined
fingerprint precheck and full-key equality predicate.

The full-key comparison is an intentional collision guard, and the cache's
bounded linear scan is appropriate for its fixed capacity. The repeated
identity rules are still a maintenance hazard: adding a cache-affecting input
requires remembering both the key representation and fingerprint encoding,
while changing the match predicate requires editing two cache operations. A
drift can cause false misses or, if the wrong comparison is weakened, reuse
semantic state under an incomplete identity.

**Recommendation:** Put fingerprint serialization on the owning
`LocalLoweringConfig`/key path and expose one private `ArtifactCacheKey::matches`
or `CacheEntry::matches` operation for the fingerprint-plus-full-key check.
Delete the duplicated predicate from `get` and `insert`, while retaining full
key verification after fingerprint comparison. Keep source text, language,
normalization mode, environment, all artifact-affecting limits, and engine
version in the identity, and preserve deterministic FIFO replacement.

**Fix Applied:** None so far.

### READ-005 — Status diagnostics expose a positional split and a redundant scope argument

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API / Conversion
- **Location:** `glass-lint-core/src/analysis/lowering/status.rs:83-143,184-263`; `glass-lint-core/src/lint/report.rs:85-95`
- **Representative callers:** `ProjectLinker::propagate_local_status`, `ProjectReportSession::status_diagnostics`, and report/output consumers that destructure the two diagnostic vectors

`AnalysisStatus` owns deterministic status entries, but its presentation API
returns an unnamed tuple of `(file diagnostics, project diagnostics)`. The
status-to-diagnostic conversion also accepts `&StatusScope` even though its
location match always produces `None`; the scope is used by the outer loop only
to decide which vector receives the diagnostic. `for_local_file` then rewrites
project scope into a file scope before the linker extends the project status.

The behavior is deliberate—file paths are carried beside file diagnostics and
parse failures are omitted here to avoid duplicate presentation—but the API
requires callers to remember tuple order and keeps scope partitioning separate
from the type that describes the diagnostic result. This is a small but
visible boundary where a later diagnostic category or scope can be added
without a named place to preserve the distinction.

**Recommendation:** Return a named private `StatusDiagnostics` aggregate or a
deterministically ordered iterator of `(StatusScope, AnalysisDiagnostic)` and
let the report layer perform the final file/project partition. Make
`IncompleteReason::diagnostic` independent of the unused scope argument, or
give it an explicit location-bearing owner if locations become part of the
diagnostic contract. Preserve BTreeSet ordering, parse-failure de-duplication,
project-to-file propagation, and stable output ordering.

**Fix Applied:** None so far.

## Systemic Themes

- **ENCAPSULATE:** Lowering completeness and derived-phase availability need
  one typed owner; status presentation should own its scope/result contract.
- **SIMPLIFY:** The final lowering transition should make policy, sealing, and
  artifact assembly explicit phases.
- **DEDUPLICATE:** Completion state, cache identity serialization, and cache
  matching predicates are represented in parallel.

## Coverage and Open Questions

Reviewed local cache keys and FIFO cache behavior, synchronized cache access,
shared semantic-artifact reconstruction, immutable local/project wrappers,
span normalization, lowerer entry points, resolved-program collection and
freeze, semantic budgets, completion capabilities, status diagnostics, and
callers in project sessions, linking, reporting, fact indexing, and effect
collection. The `LoweredSource`/`LocalArtifact`/`SharedSemanticArtifact`
wrappers were not reported as redundant because they preserve distinct
lowering, cache, and path-attachment lifecycles.

The main implementation question is whether derived-phase availability should
be attached to `SemanticArtifact` as a capability object or consumed earlier by
the fact/effect builders. Either choice must keep matcher indexes and effects
fail-closed while retaining lazy computation for complete artifacts.

## Handoff

Chunk 5 is complete. The next unreviewed chunk is **Chunk 6 — Matching**
(`CODEBASE_STRUCTURE_CORE.md` lines 424-479), covering occurrence indexes,
argument matching, evidence accumulation, and query-facing occurrence views.
