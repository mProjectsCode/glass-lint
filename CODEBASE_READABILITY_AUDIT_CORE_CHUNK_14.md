# Codebase Readability Audit

Audit scope: Chunk 14 "Local artifacts and semantic analysis" of `glass-lint-core`
(`src/analysis/local.rs`, `src/analysis/semantic/mod.rs`,
`src/analysis/semantic/budget.rs`, `src/analysis/semantic/status.rs`). Read-only;
no source changed.

## Summary

This layer is coherent where the hard parts live. The cache is genuinely
well-run: bounded FIFO-64 with a versioned fingerprint pre-check plus full-key
verification (`local.rs`), path-free cached payloads rebuilt only at hit time,
poison-recovered `Mutex`, parse failures never cached, and lazy `OnceLock`
function effects so partial analysis cannot synthesize derived state
(`SemanticArtifact::effects`, `local.rs:369,402-410`). Fact construction,
scoping, and normalization run once per source and are consumed in a single
`freeze` transition (`semantic/mod.rs:160-201,366-409`). The
`IncompleteReason::diagnostic` single-match mapping (`status.rs:209-285`) is a
good DEDUPLICATE pattern, and `AnalysisComponent`/`ResolutionKind` are justified
sub-vocabularies, not proliferation.

Findings cluster into three weak spots:

1. **Triplicated artifact containers.** `AnalyzedSource` and `LocalArtifact`
   have identical representations, joined by pure pass-through layers
   (`from_analyzed` → `into_parts`, `record_analyzed` → `record_local`,
   `semantic_handle`/`source_index` extraction for the cache). READ-001
   collapses them.
2. **Cache-key constructor implosion with test seams in production state.**
   `ArtifactCacheKey` exposes five construction paths and `SessionState` carries
   `#[cfg(test)]` fields that branch a production method. READ-002 removes the
   stateful seam; READ-004 removes a duplicated limit re-derivation.
3. **Completion model friction.** `SemanticAnalyzer` keeps a test-only knob as a
   production field (READ-003); the status model duplicates the pathless-staging
   signal across `LocalAnalysisStatus` and `StatusScope::Local` with a defensive
   demotion bucket (READ-005); and the `ParseFailure` arm of
   `IncompleteReason::diagnostic` is unreachable dead code (READ-006).

Implementation order: READ-001 first (largest surface, touches `semantic/mod.rs`,
`local.rs`, `session/artifacts.rs`, `session/mod.rs`, and several tests); then
READ-005 (touches `status.rs` and its link/report consumers); READ-002/003/004/006
are independent.

## Findings

### Artifact and cache layering (`local.rs`, `project/session/`)

#### [ ] READ-001 — `AnalyzedSource` duplicates `LocalArtifact` and the conversion chain is four pure pass-through layers

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:107-129` (`AnalyzedSource`, `into_parts`/`semantic_handle`/`source_index`), `glass-lint-core/src/analysis/local.rs:427-462` (`LocalArtifact`, `from_analyzed` at `:434-437`), `glass-lint-core/src/analysis/local.rs:241-263` (`SharedSemanticArtifact::from_analyzed`), `glass-lint-core/src/project/session/artifacts.rs:130-136` (`record_analyzed`) and `:216-227` (`insert_and_notify`), `glass-lint-core/src/project/session/mod.rs:148-157` (`complete`), `glass-lint-core/src/analysis/tests.rs:28-31`

`AnalyzedSource { source: LocatedSourceContext, semantic: Arc<SemanticArtifact> }`
and `LocalArtifact { source: LocatedSourceContext, semantic: Arc<SemanticArtifact> }`
have identical fields and no behavioral distinction; both are crate-internal
(`mod analysis` is private in `lib.rs:15`). The production path unwinds them
through a four-layer chain: `record_analyzed` (`artifacts.rs:130-136`) calls
`record_local`, which calls `LocalArtifact::from_analyzed` (`local.rs:434`), which
calls `AnalyzedSource::into_parts` (`semantic/mod.rs:118-120`) and rebuilds the
same two fields. The cache meanwhile extracts the same value a second way via
`semantic_handle()` and `source_index()` (`semantic/mod.rs:122-129`) from the
borrow, pinned by the borrow-before-consume order in `session/mod.rs:148-157`.
Tests reproduce the ceremony (`analysis/tests.rs:28-31` builds an
`AnalyzedSource` only to immediately convert it). Every future field is a
two-type maintenance cost.

**Recommendation:** Merge into `LocalArtifact` and make it the analyzer's
product: `SemanticAnalyzer::analyze_source` returns `LocalArtifact` directly;
delete `AnalyzedSource`, `into_parts`, `semantic_handle`, `source_index`,
`SharedSemanticArtifact::from_analyzed`'s `&AnalyzedSource` dependency, and
`record_analyzed` (the session calls `record_local`). Turn the cache-insert
borrow into one accessor on `LocalArtifact` (reusing `source_context().clone_lines()`
for the line index). After the merge, collapse `LocatedSourceContext`'s three
constructors (`new` test-only at `local.rs:102-108`, `with_index` at `:110-112`,
`from_normalizer` at `:114-122`) into the possession-transfer constructor that
`analyze_source` actually needs. Guardrails: keep `SharedSemanticArtifact` (the
cache payload) strictly path-free — a cached entry must never retain a
`ProjectRelativePath`; keep `ProjectModule` (`local.rs:466-500`) as the distinct
`id` + `local` linked-module concept; preserve `Send`/`Sync`/`Clone` behavior and
the borrow-cache-then-own-record ordering in `complete()`; `project/session/execution.rs`
needs no change (it only moves `ArtifactCacheKey`/`AnalyzedSource` in `LocalJobResult`).

**Fix Applied:** None so far.

#### [ ] READ-002 — `ArtifactCacheKey` constructor surface and `SessionState` test seams branch the cache-key path

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/local.rs:156-238` (`ArtifactCacheKey::new`/`with_engine_version`/`from_inputs` and `#[cfg(test)] for_engine_version`/`for_test_inputs` at `:212-237`), `glass-lint-core/src/project/session/mod.rs:37-48` (`SessionState` `#[cfg(test)]` fields), `:74-96` (`artifact_fingerprint` branches on those fields), `:395-403` (setters)

The cache-key path has five construction routes, four of which exist to smuggle
an alternate engine version or normalization mode into tests. Production
`artifact_fingerprint` (`session/mod.rs:95`) is one line, but it is wrapped in
~20 lines of `#[cfg(test)]` branching (lines 75-94) over test-only state stored
in the production `SessionState`, modified through `ProjectSession` setters
(`:395-403`). The branching re-derives the same `(environment, limits)` pair
from the analyzer each time and keeps the test hooks in a structure
(`LinterSharedConfig` adjacent) that is otherwise immutable. The
`#[cfg(test)]` `for_engine_version`/`for_test_inputs` constructors are thin
wrappers only these seams call (`local.rs:212-237`).

**Recommendation:** Keep one canonical production constructor
(`ArtifactCacheKey::new`) that internalizes the JS/TS normalization-mode choice
(the `swc-*` strings at `local.rs:167-170`), and keep exactly one structural
internal constructor with explicit `(normalization_mode, engine_version)`
parameters instead of three nested ones. Delete the stateful `cfg(test)` fields
and setters by making the test seam explicit at the call site: pass the
alternate parameters through a narrow test helper (e.g. one `#[cfg(test)]` key
builder) rather than branching a production method over stored test state.
Guardrails: fingerprint versioning, engine-version and normalization semantics
stay frozen; the `cache_and_session.rs` contract tests (`project/tests/cache_and_session.rs`)
that drive identity reuse across sessions must keep the same behavior through
the helper; production identity remains exactly `ArtifactCacheKey::new` with
`env!("CARGO_PKG_VERSION")`.

**Fix Applied:** None so far.

### Semantic analysis boundary (`semantic/mod.rs`, `semantic/budget.rs`)

#### [ ] READ-003 — `SemanticAnalyzer::name_limit` is a test-only knob stored as a production field

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:133-146` (field at `:136`, set to `MAX_NAMES` at `:144`), `:166` (the only production read), `:180-184` (`#[cfg(test)] with_name_limit`), `glass-lint-core/src/analysis/semantic/tests.rs:31-36,57-62`

`name_limit` is only ever non-default through the `#[cfg(test)]`
`with_name_limit` setter (`semantic/tests.rs:35,61`); in production it is always
`MAX_NAMES`. The identical test seam already exists in the sibling resolver API
as an explicit parameter — `resolution::collect_with_name_limit(name_limit)`
(`resolution/mod.rs:267-278`) — which is the precedent this struct should
follow instead of a stateful builder knob. The field forces every
`SemanticAnalyzer` construction to carry a value the production path cannot
change.

**Recommendation:** Drop the field; read `MAX_NAMES` where
`NameTable::with_max_entries` is built (`semantic/mod.rs:166`), and give the
name-exhaustion tests an explicit parameter, mirroring
`resolution::collect_with_name_limit` (e.g. `analyze_program_with_name_limit`),
so the cap is an argument to the analysis call, not analyzer state. Guardrails:
`MAX_NAMES` and the name-exhaustion semantics (test at
`semantic/tests.rs:26-68` asserting `semantic_name_budget_exhausted`) must
produce the same artifact/status as today; default budget/limit wiring in
`analyze_source` and `with_test_collection` is unchanged.

**Fix Applied:** None so far.

#### [ ] READ-004 — `check_fact_construction_incompleteness` duplicates the budget limit that `SemanticBudget` already owns

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:204-228` (signature at `:204-209`, limit re-read at `:211-214`), `:285-290` (call passes `resolver.budget()` alongside `resolver`), `glass-lint-core/src/analysis/semantic/budget.rs:12-45` (budget stores `limit` privately; only `used()`/`exhausted()` exposed)

The check builds `IncompleteReason::SemanticBudgetExhausted { limit, used }` by
re-reading `limits.semantic_operations()` while the `SemanticBudget` created
from that same limit sits in `resolver.budget()` (always constructed at
`semantic/mod.rs:165` and in `with_test_collection` at `:420`). The check
therefore needs its `limits` parameter only to re-derive a value the budget
already stores, and the reason-limit can silently disagree with the budget's
actual limit if budget construction ever diverges. The `semantic_operations`
limit has two owners (`AnalysisLimits` and the budget) with no `limit()`
accessor reconciling them.

**Recommendation:** Expose `SemanticBudget::limit()` and narrow the check to
`check_fact_construction_incompleteness(stream, resolver)`, deriving the budget
via `resolver.budget()` and building the reason from `budget.limit()` /
`budget.used()`. Delete the `limits` parameter and the call-site duplication at
`semantic/mod.rs:285-290`. Guardrails: the emitted reason must report the exact
same limit and used counts seen by the budget; budget construction points
(`analyze_program`, `with_test_collection`, `resolution` tests) continue to
derive from `AnalysisLimits::semantic_operations()` or `UNLIMITED_SEMANTIC_OPS`.

**Fix Applied:** None so far.

### Status model (`semantic/status.rs`)

#### [ ] READ-005 — `LocalAnalysisStatus` and `StatusScope::Local` double-encode pathless staging and force a defensive demotion bucket

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/semantic/status.rs:85-91` (`StatusScope::Local`), `:106-125` (`LocalAnalysisStatus` newtype, pinned-scope `record` at `:110-112`, `materialize_file` at `:119-124`), `:141-156` (`AnalysisStatus::materialize_file` Local→File rename), `:158-187` (`diagnostics`; Local demotion to the project bucket at `:176-184`)

The pathless-staging invariant is encoded twice: the `LocalAnalysisStatus`
newtype pins `record` to `StatusScope::Local`, and the `Local` scope variant
itself carries the same "no path yet" meaning into the shared `AnalysisStatus`.
That leaks into `AnalysisStatus::diagnostics`, which must demote any surviving
`StatusScope::Local` entry into the *project* bucket with a defensive comment
(`status.rs:177-184`) because every production materialization maps Local→File
(`linker/mod.rs:91`, `analysis/project/model.rs:267`), leaving `Local` as a
representation that exists only where materialization is absent. A single
staging signal is being maintained in two forms.

**Recommendation:** Make `LocalAnalysisStatus` the sole owner of staging by
storing raw `IncompleteReason`s (a scoped entry set or wrapper set) instead of
an `AnalysisStatus`, and have `materialize_file(path)` produce the path-scoped
`AnalysisStatus` by inserting `StatusScope::File(path)` entries. Delete
`StatusScope::Local` and the `Local`/`Project` demotion arm in
`diagnostics()`, so `AnalysisStatus` never contains a pathless entry and the
two `record` methods (`AnalysisStatus::record(scope, ..)` for File/Project vs.
`LocalAnalysisStatus::record(reason)` pinned) no longer overlap. Guardrails:
materialization must stay total for the three consumers
(`linker/mod.rs:86-102`, `project/model.rs:259-277`, report assembly via
`status_snapshot` at `model.rs:421`), so the same file/project diagnostic
vectors and completion semantics (`is_complete`, `semantic/tests.rs` and
`status/tests.rs:56-69`) hold unchanged.

**Fix Applied:** None so far.

#### [ ] READ-006 — `IncompleteReason::ParseFailure` diagnostic arm is unreachable; the skip is duplicated at the consumer

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/semantic/status.rs:173-175` (skip in `AnalysisStatus::diagnostics`), `:219-222` (dead `ParseFailure` arm in `IncompleteReason::diagnostic`), record sites `glass-lint-core/src/lint/report/mod.rs:79-88` and parser presentation `glass-lint-core/src/parse.rs:47-52`

`AnalysisStatus::diagnostics` skips `IncompleteReason::ParseFailure` before ever
calling `entry.reason.diagnostic()` (`status.rs:173-176`), so the `ParseFailure`
arm inside `IncompleteReason::diagnostic` (`status.rs:219-222`) is dead; the
classification "parse presentation is handled by the `ParseDiagnostic` owner"
is enforced twice in two places (a `matches!` in the caller plus a full
diagnostic arm that can never run). Any future direct caller of
`reason.diagnostic()` on a recorded parse failure would silently produce the
formatted diagnostic while the aggregation path goes out of its way not to.

**Recommendation:** Move the skip decision onto the reason itself so the
classification lives once. Give `IncompleteReason` a single predicate or make
`diagnostic()` fallible (e.g. return `Option<AnalysisDiagnostic>` with
`ParseFailure => None`), and have `AnalysisStatus::diagnostics` branch on that
enumeration; delete the `matches!` skip and the dead arm body. Guardrails: the
authorized `ParseDiagnostic`/`ParseFailureKind` presentation (`parse.rs:47-52`,
`report/mod.rs:79-88`, `session/artifacts.rs:121-127,173-213`) remains the
sole parser reporting path, and `ParseFailure` keeps making analysis
incomplete (it is deliberately a completion side channel, per the comment at
`status.rs:167-172`).

**Fix Applied:** None so far.

## Systemic Themes

- **Completion models are reinvented per phase.** `AnalysisCompletion`
  (`semantic/mod.rs:245-295`) pairs `LocalAnalysisStatus` with
  `DerivedPhaseCapabilities`; `FlowCompletion` (`flow/mod.rs:24-69`) is a
  reason bitmask; projection has `ProjectionCompletion`
  (`project/projection/outcome.rs`). They share the "complete iff no bounded
  resource reason is set" contract but use three mechanisms with three
  recording vocabularies. Not a chunk-14 finding (flow/projection are owned by
  other chunks), but READ-003/004/005 should not entrench the third one further.
- **The status vocabulary is not over-proliferated.** `AnalysisComponent` and
  `ResolutionKind` (`status.rs:10-36`) each deduplicate a diagnostic mapping
  shared by multiple subsystems (effects/flow/linking; unsupported/outside
  requests), and `IncompleteReason`'s single `diagnostic()` match is the right
  single-source mapping. Only the staging duplication in READ-005 and the dead
  arm in READ-006 are warts.
- **Cache lifecycle invariants are held and worth preserving.** Bounded 64-entry
  FIFO with fingerprint pre-check + full-key verification (`local.rs:329-358`),
  poisoned-mutex recovery (`:291-306`), no-caching-of-parse-failures
  (`session/mod.rs:159-161` records failure only), and path-free cached entries
  rebuilt on hit (`get_local`, `local.rs:309-315`) are the fail-closed behavior
  READ-001 must not disturb.
- **Pure pass-through chains recur beyond this chunk.** `record_analyzed` →
  `record_local` → `from_analyzed` → `into_parts` (README-001) mirrors the
  `lower()` test helper (`artifacts/tests.rs:8-14`) that re-wraps the same
  value; consolidating the chain removes both.

## Open Questions

- **Should the line index travel with facts?** `SharedSemanticArtifact`
  (`local.rs:240-263`) stores `Arc<SourceLineIndex>` next to the semantic
  artifact, and `SemanticArtifact` stores none; the line index is content-derived
  like the facts are. If the line index moved into the artifact, the cache
  payload and `LocatedSourceContext` would share one owner and READ-001's
  accessor count drops further. Not a finding: the index currently stays with
  the report-facing `LocatedSourceContext`, and the artifact must remain
  path-free (line index is not a path, so the merge is plausible but not
  required).
- **`ArtifactCacheKey` full-key retention vs. fingerprint-only identity.**
  `CacheEntry` clones the entire `SourceText` for collision verification
  (`local.rs:146-154,265-270`). The versioned XXH3 fingerprint covers every key
  dimension, so full-key equality is verification-only; 64 large sources are
  retained per cache. Whether the key should be reduced to
  fingerprint-plus-verified-hash identity instead is a memory/behavior
  tradeoff, not a readability issue; left unresolved since READ-002 does not
  require it.
- **`StatusScope::Local` removal scope.** READ-005's viability depends on no
  consumer recording `StatusScope::Local` directly on an `AnalysisStatus`;
  today only `LocalAnalysisStatus::record` does (`status.rs:110-112`). Confirm
  at `linker/mod.rs`, `project/model.rs`, and report assembly after the change
  that no direct `Local` scope remains.

## Coverage

Chunk 14 sources reviewed:

- `glass-lint-core/src/analysis/local.rs` (503 lines): `ArtifactFingerprint`,
  `ArtifactCacheKey` constructor surface and fingerprint, `LocatedSourceContext`,
  `SharedSemanticArtifact`, `CacheEntry`, `ArtifactCache`/`ArtifactCacheHandle`
  (FIFO-64, matches, eviction), `SemanticArtifact` (lazy `OnceLock` effects,
  status, export origins), `LocalArtifact`, `ProjectModule`.
- `glass-lint-core/src/analysis/semantic/mod.rs` (434 lines): `InvalidParserSpan`,
  `ParserSpanKey`, `SpanNormalizer`, `AnalyzedSource`, `SemanticAnalyzer`,
  `ResolvedProgram` collect/seal/freeze, `AnalysisCompletion` and the four
  `check_*` incompleteness detectors.
- `glass-lint-core/src/analysis/semantic/budget.rs` (51 lines): `SemanticBudget`
  charge/exhaust/used, `UNLIMITED_SEMANTIC_OPS`.
- `glass-lint-core/src/analysis/semantic/status.rs` (288 lines):
  `AnalysisComponent`, `ResolutionKind`, `IncompleteReason` (18 variants and the
  single diagnostic mapping), `StatusScope`, `StatusEntry`, `AnalysisStatus`,
  `LocalAnalysisStatus`, `materialize_file`, `diagnostics`.
- `glass-lint-core/src/analysis/semantic/tests.rs` (163 lines), `.../status/tests.rs`
  (69 lines), `.../local/tests.rs` (112 lines): completion, budget/name/invalid-span
  exhaustion, cache hit/evict/replace/miss contracts, Send+Sync.

Representative callers traced:

- `lint/linter.rs:130-141,204-206` — `begin_project` seeds `SessionState` with the
  shared cache handle; `lint_source`/`lint_batch` (`:222-266`,
  `lint/batch.rs:283`) run `run_single_source` → session.
- `project/session/mod.rs:37-96,118-170` — cache-key construction, prepare/complete
  with borrow-cache-then-own-record ordering.
- `project/session/artifacts.rs:107-227` — `record_analyzed`/`record_local`,
  `insert_and_notify`, `into_link_input` (parse-failure split, authored-request
  validation).
- `project/session/execution.rs:17-35` — `LocalJob`/`LocalJobResult` carry the
  prepared `ArtifactCacheKey`.
- `project/tests/cache_and_session.rs:65-330` — cross-session cache reuse, capacity.
- `analysis/project/linker/mod.rs:86-102` — propagates per-module
  `materialize_file` status into the project aggregate.
- `analysis/project/model.rs:259-277,421` — `single`, status snapshot.
- `analysis/project/linker/graph.rs:37-101`, `linker/export.rs:62-167`,
  `project/projection/outcome.rs:49-108` — `BudgetExhausted`/resolution/link
  status recording and `AnalysisComponent`/`ResolutionKind`/`StatusScope` use.
- `lint/report/mod.rs:79-88,111-121` — `RecordParseFailure`, status diagnostics
  and `is_complete` readiness feeding `report/summary.rs:35`.

No source, test, configuration, or dependency files were modified. `git status`
confirms the only change is the addition of this file
(`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_14.md`); the pre-existing untracked
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_*.md` files from parallel sessions were
left untouched.