# Codebase Readability Audit

## Summary

Chunk 5 owns the parser-to-artifact transition, reusable local-artifact
cache, completion status, and the project-facing attachment of source context.
The consuming semantic phases, artifact-independent cache key, collision
verification, lazy effect ownership, and path-local cache reattachment are
appropriate boundaries. Three current opportunities remain: source
coordinates are indexed twice on every cache miss, scope failures update
status without going through the same derived-phase invalidation policy as
other failures, and local cache tests repeat the same raw artifact fixture.

## Findings

### Source-coordinate ownership

#### [ ] READ-056 — Reuse the span normalizer’s source-line index

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Performance
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:55-83, 169-177`; `glass-lint-core/src/analysis/local.rs:101-110, 121-123`

`SemanticAnalyzer::analyze_source` creates a `SpanNormalizer`, whose
`SpanNormalizer::new` allocates a `SourceLineIndex`, and then constructs the
returned `LocatedSourceContext` with `LocatedSourceContext::new`, which builds
another `SourceLineIndex` from the same `SourceText`. The normalizer’s index is
discarded after analysis while the second index is retained by the analyzed
artifact and later shared by cache hits. The source text allocation is
Arc-backed, but the line-start vector, index metadata, and construction work
are still duplicated for every cache miss.

**Recommendation:** Add a narrow owner-facing conversion or accessor that
transfers or clones the normalizer’s existing `Arc<SourceLineIndex>` into
`LocatedSourceContext::with_index`, preserving the source path from the
`SourceFile`; avoid exposing the normalizer’s representation to callers.
Delete the second `SourceLineIndex::from_text` path from `analyze_source` and
keep the cache’s path reattachment behavior unchanged. Preserve span
normalization against the exact authored source, UTF-8 boundary validation,
line-index sharing across cached artifacts, and deterministic locations.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. Reuse the normalizer-owned
index through a narrow conversion boundary; do not expose parser internals or
change cache reattachment ownership.

### Semantic completion policy

#### [ ] READ-057 — Centralize capability invalidation for structural failures

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:230-277`; capability owner in `glass-lint-core/src/analysis/mod.rs:39-85`; scope validity in `glass-lint-core/src/analysis/scope/build/freeze.rs:13-57` and `glass-lint-core/src/analysis/scope/graph.rs:183-203`

`AnalysisCompletion` has two different incomplete-analysis transitions.
`record_fact_failure` records the reason and disables all derived phases,
whereas `record_scope_issue` records `ScopeShapeMismatch` but leaves
`DerivedPhaseCapabilities` enabled. The scope freeze path marks the frozen
graph invalid when collection and planned shapes diverge, so the artifact can
simultaneously report an incomplete scope status while advertising matcher
indexes/effects as available. The policy is split between status recording and
capability invalidation, making future structural-failure call sites easy to
implement inconsistently.

**Recommendation:** Give `AnalysisCompletion` one private incomplete-transition
operation that records the reason and applies the appropriate capability
policy, then route scope, fact, parser-span, name, and future structural
failures through it. Scope-shape invalidity should fail closed for derived
indexes/effects while retaining the raw facts and diagnostic status for
reporting. Preserve independent complete-witness handling, deterministic
status deduplication, the existing scope-shape diagnostic, and the distinction
between a disabled derived phase and an empty successful result; add a focused
test that a scope-shape failure cannot be observed as an available derived
phase.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. Scope-shape failure is a
structural failure and must invalidate derived capabilities while retaining
raw facts and diagnostics.

### Local cache test fixtures

#### [x] READ-058 — Centralize the repeated empty cached-artifact fixture

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Testing
- **Location:** `glass-lint-core/src/analysis/local.rs:500-505, 537-637`

The cache tests repeatedly spell out the same `SharedSemanticArtifact` value:
an `Arc` around an empty `SemanticArtifact::from_analysis` with an empty fact
set, unlimited effect limit, enabled capabilities, and default status. The
insert/hit, eviction, replacement, and miss tests therefore duplicate the
artifact’s construction contract rather than stating the cache behavior they
are testing. If the semantic artifact constructor gains a required field,
these fixtures can drift independently or make the cache tests fail for setup
reasons unrelated to cache semantics.

**Recommendation:** Add one private test helper such as
`empty_shared_artifact()` beside `test_key` and use it in all cache tests;
retain per-test keys and assertions as the behavioral inputs. Keep the helper
constructing the real production `SharedSemanticArtifact` shape, preserve the
Arc/send-sync and FIFO/replacement behavior under test, and do not hide the
cache’s collision-key or eviction assertions behind a higher-level mock.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. The helper belongs in test code
and should construct the real bounded artifact, not a mock or an unbounded
substitute.

**Fix Applied:** Added one `empty_shared_artifact()` test helper and reused it
across cache hit, miss, eviction, and replacement tests. Cache keys and all
behavioral assertions remain per-test. Verified with `make fmt && make ci`.

## Systemic Themes

- Source-coordinate ownership should have one index construction per analyzed
  source; cache reuse should extend that ownership rather than reconstitute
  equivalent location state.
- Completion status and derived-phase availability are one semantic policy.
  Structural invalidity must not be represented by a status-only side channel
  whose capabilities remain enabled.
- Test fixtures should centralize stable production-object construction while
  leaving cache keys, limits, eviction order, and observable behavior explicit.
- The parser-to-artifact consuming transition, matcher-independent semantic
  model, collision-checked bounded FIFO cache, lazy effect derivation, and
  path-specific source attachment were reviewed and retained as necessary
  architecture. `AnalyzedSource` and `LocalArtifact` were not collapsed:
  their producer-to-project and path-attachment transitions are distinct even
  though their storage is intentionally similar.

## Open Questions

- None blocking these findings. No earlier Chunk 5 audit artifact was present;
  the preceding audit chain ends at READ-055.

## Coverage

Reviewed only Chunk 5, “Local artifacts and semantic analysis,” from
`CODEBASE_STRUCTURE_CORE.md`: local artifact/cache keys and handles, source
location context, semantic artifact ownership and lazy effects, parser-to-
artifact analysis, bounded semantic budgets, completion capabilities and
status diagnostics, resolved-program sealing, project-module attachment, and
the representative session/cache tests and callers. The root and core
architecture documents, testing/contribution guidance, current audit chain,
and relevant project-session callers were inspected. The focused semantic
test suite passed: `cargo test -p glass-lint-core analysis::semantic --lib`
(9 passed). No source, test, configuration, dependency, or other
documentation files were changed; this chunk audit file was updated only with
review dispositions. The next chunk is Chunk 6, “Matching,” which should continue finding
IDs at READ-059.
