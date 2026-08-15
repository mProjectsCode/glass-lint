# Codebase Readability Audit — glass-lint-core Chunk 14: Local artifacts and semantic analysis

## Summary

Chunk 14 owns the matcher-independent local semantic pipeline: `analysis::local`
(retained artifacts, the bounded fingerprint-keyed cache, path attachment) and
`analysis::semantic` (the parser-to-artifact boundary, `SpanNormalizer`,
`SemanticAnalyzer`, `ResolvedProgram`, plus the `budget` and `status` modules).

The chunk's boundaries are healthy: SWC types stay inside `semantic`,
`SemanticArtifact` exposes only narrow accessors (`facts`, `effects`, `status`,
`interface`, `export_origin`), the cache stores only reusable semantic state and
attaches paths at consumption time, and the `LocalAnalysisStatus → AnalysisStatus`
materialization is a clean one-way scope transition. The main problems are
concentrated in the completeness/status vocabulary: two overlapping budget-exhaustion
variants (one effectively unreachable), a `LocalAnalysisStatus` newtype that leaks
the general `AnalysisStatus` surface through `Deref`, and a single-variant enum used
as a constant payload. Secondary issues are an inconsistent cross-module field
access (`resolver.budget`) and an asymmetric constructor surface shared by
`SpanNormalizer` and `LocatedSourceContext`.

No source, test, config, or documentation file was modified; only this audit file
was created.

## Findings

### analysis/semantic status and completion

#### [x] READ-001 — Two overlapping "facts budget exhausted" variants, one unreachable

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:207-238`, `glass-lint-core/src/analysis/semantic/status.rs:50-63`, `glass-lint-core/src/analysis/semantic/status.rs:254-264`

`IncompleteReason` carries two variants for the same concept: `SemanticBudgetExhausted { limit, used }`
(recorded at `semantic/mod.rs:213-218`) and `BudgetExhausted { component: AnalysisComponent::Facts, limit, observed }`
(recorded at `semantic/mod.rs:230-236`). They map to distinct public diagnostic codes
`semantic_step_budget_exhausted` (`status.rs:238-241`) and `semantic_budget_exhausted`
(`status.rs:254-264`). The final branch of `check_facts_budget` is unreachable:
`FactStream::append` sets `valid = false` only together with `mark_budget_exhausted()`
(`analysis/facts/stream.rs:250-258`), so `!stream.is_structurally_valid()` implies
`stream.budget_exhausted()`, which returns earlier (`semantic/mod.rs:219-223`). Every
other producer of `BudgetExhausted` uses the `Effects`/`Flow`/`Linking` components
(`linker/graph.rs:94`, `linker/export.rs:66`, `projection/outcome.rs:55,67`); the
`Facts` component is constructed nowhere else. A reader must hold all of this in mind
to understand that local fact exhaustion is reported by one variant while its apparent
twin cannot fire, and the `semantic_budget_exhausted` code is not producible by local
analysis. The `FactCapacityExhausted { limit: stream.max_facts() }` mapping
(`semantic/mod.rs:219-223`) is additionally confusing because it is driven by the
stream's `BudgetExhausted` issue flag, not by an explicit capacity field.

**Recommendation:** Delete the dead branch at `semantic/mod.rs:230-236` and
consolidate local budget reporting onto one variant, either folding
`SemanticBudgetExhausted` into `BudgetExhausted { component: Facts, .. }` or keeping
`SemanticBudgetExhausted` as the sole local budget variant and removing the `Facts`
arm from `AnalysisComponent::budget_diagnostic` (`status.rs:201-222`). Guardrails:
the two codes are public report schema, so whichever variant you drop, update its
assertions in the same change: dropping `SemanticBudgetExhausted` (option A) affects
`semantic_step_budget_exhausted`, asserted at `project/tests/status_policy.rs:251`,
`project/tests/mod.rs:170`, `project/tests/cache_and_session.rs:147`,
`cli/src/output/tests.rs:77`, and `project/types/report/code/tests.rs:18`; dropping
the `Facts` arm (option B) affects `semantic_budget_exhausted`, asserted at
`analysis/semantic/status/tests.rs:20`, `project/report/tests.rs:167`, and
`project/types/report/code/tests.rs:13`, and constructed at
`lint/report/files/tests.rs:8`. Under option B, removing the `Facts` arm also removes
`AnalysisComponent::Facts`, whose only remaining reference is the status unit test.
Update every assertion and any harness fixtures in the same change, and preserve
fail-closed behavior — an exhausted budget must still yield an incomplete
`AnalysisStatus`, never a successful-empty result.

**Fix Applied:** Chunk 01 (commit 87f4d896) already deleted the dead `BudgetExhausted { Facts }` branch from `check_facts_budget` and removed the `Facts` arm from `AnalysisComponent::budget_diagnostic`, leaving `SemanticBudgetExhausted` as the sole local budget variant. This chunk finished option B's guardrail: removed the now-unproducible `DiagnosticKind::FactsBudgetExhausted` (`semantic_budget_exhausted`) from the report schema (`code.rs`) and updated its test assertions in `project/report/tests.rs`, `lint/report/files/tests.rs`, and `code/tests.rs` to use `FactCapacityExhausted` instead. Fail-closed behavior is preserved: `FactCapacityExhausted` still reports an incomplete status for an exhausted fact stream.

#### [x] READ-002 — `LocalAnalysisStatus` newtype leaks the general `AnalysisStatus` surface via `Deref`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/semantic/status.rs:114-136`

`LocalAnalysisStatus(AnalysisStatus)` is a newtype whose purpose is to constrain a
reusable local artifact's completeness record to `StatusScope::Local` entries: its
inherent `record` hardcodes `StatusScope::Local` (`status.rs:118-120`) and its only
transition is `materialize_file` (`status.rs:122-127`). The one-way `Deref`
(`status.rs:130-136`) exposes the entire read-only `AnalysisStatus` surface —
`is_complete` (`status.rs:147-149`), `diagnostics` (`status.rs:169-198`, which
silently buckets `Local`-scoped entries as project-level), and a second
materialization under a near-identical name `materialize_local_file`
(`status.rs:152-167`). Callers therefore have two reachable spellings for the same
transition (`module.local().status().materialize_file(...)` at
`analysis/project/linker/mod.rs:103` versus the Deref-reachable
`materialize_local_file`), and the "Local-only scope" contract of the newtype is
enforced only by reader discipline. The absence of `DerefMut` keeps mutation safe
today, so this is an API-surface and naming problem rather than an active invariant
violation.

**Fix Applied:** Removed the `Deref` impl from `LocalAnalysisStatus` and added a narrow inherent `is_complete()` (delegating to `AnalysisStatus::is_complete`), keeping `record` and `materialize_file` as the only mutation/transition operations. Renamed `AnalysisStatus::materialize_local_file` to `materialize_file` so one canonical name exists per concept, and updated the three test callers (`status/tests.rs`, `analysis/semantic/tests.rs`) and the `LocalAnalysisStatus` delegation.

**Recommendation:** Remove the `Deref` impl and give `LocalAnalysisStatus` narrow
inherent methods for every genuinely needed query (at minimum `is_complete()`), while
keeping `record(StatusScope::Local, ..)` and `materialize_file` as the only mutation
and scope-transition operations. Consider renaming `AnalysisStatus::materialize_local_file`
to `materialize_file` so one canonical name exists per concept. Guardrails: `AnalysisStatus`
remains the materialized project/report type used by the linker
(`analysis/project/model.rs:427 status_snapshot`) and report session
(`lint/report/mod.rs:74`); keep the `is_complete` semantics (empty entry set equals
complete) and the `materialize_file` path-rewrite behavior unchanged.

**Fix Applied:** None so far.

#### [ ] READ-003 — Single-variant `ModuleInterfaceKind` used as a constant payload

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/semantic/status.rs:18-21`, `glass-lint-core/src/analysis/semantic/status.rs:68-70`, `glass-lint-core/src/analysis/semantic/status.rs:269-274`

`ModuleInterfaceKind` has exactly one variant (`CommonJsExports`), and
`IncompleteReason::UnsupportedModuleInterface { kind: ModuleInterfaceKind }` is
constructed at a single site with that constant (`analysis/project/linker/mod.rs:108-113`);
the `diagnostic()` arm (`status.rs:269-274`) also hardcodes the only variant. The enum
plus its payload field therefore carry no information today — a one-field wrapper whose
single consumer immediately discards the variability — and the speculative `kind`
dimension obscures rather than documents the outcome.

**Recommendation:** Since only one kind exists and its only producer hardcodes it,
make `UnsupportedModuleInterface` a unit variant and drop `ModuleInterfaceKind` (or
add the second kind that the abstraction anticipates). Guardrails: keep the diagnostic
code and message identical (`status.rs:271-274`); the variant is part of the shared
`IncompleteReason` vocabulary used by the linker and report phases, so update every
`match`/constructor in the same change and preserve fail-closed semantics.

**Fix Applied:** None so far.

#### [ ] READ-004 — Completion-assessment helpers misnamed and reading `Resolver.budget` across module boundary

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:207-238`, `glass-lint-core/src/analysis/semantic/mod.rs:273-277`, `glass-lint-core/src/analysis/semantic/mod.rs:299`, `glass-lint-core/src/analysis/resolution/mod.rs:171-173`

`check_facts_budget` (`semantic/mod.rs:207`) is named for one condition but returns
five distinct exhaustion reasons (step budget, fact capacity, path capacity, value
arena, and structural invalidity — the last is the dead `Facts`-component branch from
READ-001), and `AnalysisCompletion::record_fact_failure`
(`semantic/mod.rs:273-277`) also records name exhaustion and invalid-parser-span
reasons that are not "fact failures." The budget is threaded as a parameter yet the
single call site reads it from a sibling module's field: `check_facts_budget(..., resolver.budget)`
(`semantic/mod.rs:299`) reaches into `Resolver`'s `pub(super) budget` field
(`resolution/mod.rs:171-173`) instead of using a method, so the owner of the shared
`SemanticBudget` state is unclear from the call.

**Recommendation:** Rename the helpers to reflect the full condition set (e.g.
`check_fact_construction_incompleteness`, and a name for `record_fact_failure` that
covers name-exhaustion and invalid-parser-span reasons), and add a
`Resolver::budget()` accessor so the call site at `semantic/mod.rs:299` passes
`resolver.budget()` instead of reaching into the sibling module's `pub(super)` field.
Guardrails: preserve the current check ordering — step-budget exhaustion is reported
before capacity (`semantic/mod.rs:213-223`) — since that precedence is observable in
reported diagnostics, and do not pull budget state into a new owner; the shared
`SemanticBudget` stays the reference already created by
`SemanticAnalyzer::analyze_program` (`semantic/mod.rs:168`) and threaded through
`ResolvedProgram::collect` into `Resolver`.

**Fix Applied:** None so far.

### analysis/local and analysis/semantic construction

#### [ ] READ-005 — `SpanNormalizer` and `LocatedSourceContext` split the shared line-index invariant with an asymmetric constructor surface

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:55-78`, `glass-lint-core/src/analysis/semantic/mod.rs:96-102`, `glass-lint-core/src/analysis/local.rs:96-112`

`SpanNormalizer` (`Arc<SourceLineIndex>` + SWC `start`) and `LocatedSourceContext`
(`Arc<SourceLineIndex>` + `path`) each own the per-source coordinate state, and the
production construction chain crosses them with a consuming `into_source_context`
(`semantic/mod.rs:96-102`) — the `start` offset is dropped and the same `Arc` reused.
The constructors are inconsistent: `SpanNormalizer::with_index` takes
`SourceLineIndex` by value (`semantic/mod.rs:70-78`) while `LocatedSourceContext::with_index`
takes `Arc<SourceLineIndex>` (`local.rs:110-112`), and "build an index from source
text" exists in three shapes (`SourceLineIndex::from_text`, `SpanNormalizer::new`,
`LocatedSourceContext::new` at `local.rs:103-108`). A reader cannot tell from the
types where the single shared line index is created, who owns it, or why one
constructor takes an `Arc` and the other does not.

**Recommendation:** Unify the shared state on one owner — build `Arc<SourceLineIndex>`
once at the parse step and have both types take it by `Arc` — and make the
normalizer-to-context transition an explicit constructor or `From` on
`LocatedSourceContext` instead of a field-dropping method. Guardrails: keep the two
responsibilities distinct (SWC `BytePos` normalization vs authored display-range
reporting) and do not collapse them into one type; preserve the invariant that
`SpanNormalizer` and `LocatedSourceContext` derived from the same source share one
line-index allocation (asserted by `analysis/local/tests.rs:48-54`).

**Fix Applied:** None so far.

## Systemic Themes

- **One shared completeness vocabulary, scoped and materialized.** `IncompleteReason`
  deliberately aggregates local, linking, projection, and report reasons
  (`ParseFailure`, `UnsupportedResolution`, `EvidenceCapacityMismatch`,
  `RuleSelectionInvalid`, ...) under one `AnalysisStatus` keyed by `StatusScope`, so
  that project completion stays total and fail-closed. Preserve this aggregation; only
  the local-phase vocabulary (READ-001, READ-003) needs tightening.
- **Status materialization is the only scope transition.** `LocalAnalysisStatus::materialize_file →
  AnalysisStatus` is the sole allowed Local→File rewiring, and the
  `ParseFailure` skip in `AnalysisStatus::diagnostics` (`status.rs:184-186`) keeps
  parser presentation out of the completion channel. Keep this one-way.
- **SWC isolation and cache discipline are strong.** `SpanNormalizer`, `ResolvedProgram`,
  and `SemanticBudget` keep parser and AST details inside `semantic`; the cache stores
  only reusable `SharedSemanticArtifact` state and re-attaches path/line context at hit
  time (`local.rs:244-252`, `local.rs:299-311`). These boundaries should be retained.
- **Fine visibility tiers with one inconsistency.** Accessor visibility is mostly
  consistent (`pub(in crate::analysis)` for facts/effects/status/export_origin), but
  `LocalArtifact::interface` and `LocalArtifact::source_context` are `pub(crate)`
  (`local.rs:429-435`) while their siblings are narrower — align them.
- **Cross-chunk reach-in.** `semantic/mod.rs` reads `Resolver.budget` (a `pub(super)`
  field owned by `analysis::resolution`) — the same pattern recurs in
  `analysis/facts/mod.rs` (`self.resolver.budget.try_charge()`). A `budget()` accessor
  would remove the representation leak without changing ownership.

## Open Questions

1. **Resolved — cache memory retention is deliberate and bounded.** `ArtifactCacheKey`
   retains a full `SourceText` clone (`local.rs:187`), each in-flight `LocalJob` carries
   the key (`project/session/execution.rs:24`), and the cached artifact's own
   `Arc<SourceLineIndex>` also holds the source text (`diagnostic.rs:54`), so up to
   `MAX_ENTRIES` (64) plus in-flight sources are duplicated. The full-key check is an
   explicit design choice: `CacheEntry` documents that "a fingerprint match is not a
   hit until the full key matches" (`local.rs:255-266`), deliberately rejecting
   fingerprint-only identity to avoid any collision risk. Retention is acceptable under
   that documented boundedness contract; do not change it without the fail-closed
   full-key check in mind.
2. **Resolved — the split is intentional.** `ParserSpanKey { lo, hi }`
   (`semantic/mod.rs:37-49`) stores unvalidated SWC coordinates and is used only as a
   hash-map identity key (`analysis/resolution/mod.rs:121-133`,
   `analysis/facts/call_results.rs:11`); it is never converted to a `ByteRange` at
   consumption. Validated `ByteRange`s are produced separately via
   `SpanNormalizer::normalize`, which rejects dummy/out-of-range spans that
   `ParserSpanKey` deliberately accepts, so a `From<Span>`-style conversion to
   `ByteRange` is not a viable single owner.
3. **Resolved — the dual construction path is test-only.** Production uses only
   `SpanNormalizer::with_index` (`semantic/mod.rs:197`); `new` (`semantic/mod.rs:63-68`)
   and `Default` (`semantic/mod.rs:104-108`) are used only by test modules (semantic,
   analysis, matching-arguments, linker-graph, and resolution tests). `with_index` is
   already the canonical constructor; `new`/`Default` can stay as test conveniences or
   be made `#[cfg(test)]`.
4. **Resolved — the three flags are always coupled today.** `disable_derived_phases`
   (`analysis/mod.rs:68-73`) turns off `fact_index`, `effects`, and `export_origins`
   together, the only construction is `enabled()` (all three on), and no path disables
   a single phase. A single all-or-nothing availability would be simpler today; keep
   the per-field shape only if independent per-phase gating is planned.

## Coverage

Reviewed in full:

- `glass-lint-core/src/analysis/local.rs` (artifact, cache key/handle/cache,
  `LocatedSourceContext`, `SemanticArtifact`, `LocalArtifact`, `ProjectModule`)
- `glass-lint-core/src/analysis/semantic/mod.rs` (`InvalidParserSpan`,
  `ParserSpanKey`, `SpanNormalizer`, `AnalyzedSource`, `SemanticAnalyzer`,
  `check_*` helpers, `AnalysisCompletion`, `ResolvedProgram`)
- `glass-lint-core/src/analysis/semantic/budget.rs` (`SemanticBudget`)
- `glass-lint-core/src/analysis/semantic/status.rs` (`AnalysisComponent`,
  `ModuleInterfaceKind`, `ResolutionKind`, `IncompleteReason`, `StatusScope`,
  `StatusEntry`, `AnalysisStatus`, `LocalAnalysisStatus`)
- Chunk unit tests: `analysis/local/tests.rs`, `analysis/semantic/tests.rs`,
  `analysis/semantic/status/tests.rs`

Traced representative callers and dependent modules:

- `analysis/mod.rs` (re-exports, `DerivedPhaseCapabilities`)
- `analysis/resolution/mod.rs` (`Resolver.budget`, `ParserSpanKey`, `SpanNormalizer`)
- `analysis/facts/stream.rs` (issue flags, `append` invariant sites)
- `analysis/facts/mod.rs`, `analysis/facts/call_results.rs`
- `analysis/project/linker/mod.rs`, `analysis/project/model.rs`,
  `analysis/project/linker/graph.rs`, `analysis/project/linker/export.rs`,
  `analysis/project/projection/outcome.rs`
- `project/session/mod.rs`, `project/session/artifacts.rs`,
  `project/session/execution.rs`
- `lint/report/mod.rs`, `lint/report/diagnostics.rs`
- `project/types/report/code.rs` (diagnostic codes)
- Status-policy and cache tests: `project/tests/status_policy.rs`,
  `project/tests/cache_and_session.rs`, `project/report/tests.rs`
