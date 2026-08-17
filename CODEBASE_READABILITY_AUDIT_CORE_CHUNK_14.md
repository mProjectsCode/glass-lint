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
good DEDUPLICATE pattern for the presentable reasons (READ-006 replaces only the
dead `ParseFailure` arm), and `AnalysisComponent`/`ResolutionKind` are justified
sub-vocabularies, not proliferation.

Findings cluster into three weak spots:

1. **Triplicated artifact containers.** `AnalyzedSource` and `LocalArtifact`
   have identical representations, joined by pure pass-through layers
   (`from_analyzed` → `into_parts`, `record_analyzed` → `record_local`,
   `semantic_handle`/`source_index` extraction for the cache). READ-001
   collapses them.
2. **Constructor and limit implosion.** `ArtifactCacheKey` exposes five
   construction paths and `SessionState` carries `#[cfg(test)]` fields that
   branch a production method (READ-002); the incompleteness check re-derives
   the budget limit the budget already owns (READ-004).
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
borrow, pinned by the borrow-before-consume order in `session/mod.rs:148-157`;
those two accessors exist only because `SharedSemanticArtifact` (in `local.rs`)
cannot reach `AnalyzedSource`'s private fields across the module boundary. Tests
reproduce the ceremony (`analysis/tests.rs:28-31,98-101` builds an
`AnalyzedSource` only to immediately convert it). Every future field is a
two-type maintenance cost.

**Recommendation:** Merge into `LocalArtifact` and make it the analyzer's
product: `SemanticAnalyzer::analyze_source` returns `LocalArtifact` directly;
delete `AnalyzedSource`, `into_parts`, `semantic_handle`, `source_index`, and
`record_analyzed` (the session calls `record_local`). Retarget
`SharedSemanticArtifact::from_analyzed` to `SharedSemanticArtifact::from_local(&LocalArtifact)`
— both types live in `local.rs`, so it can read the fields directly
(`semantic.clone()` plus `source_context().clone_lines()`, `local.rs:132-134`),
which removes the two accessor round-trips entirely. After the merge,
`LocatedSourceContext::from_normalizer` (`local.rs:114-122`) is a pure
extraction wrapper that exists only to unpack `SpanNormalizer::into_lines()` for
`analyze_source` (`semantic/mod.rs:198`); fold it into the call site (`let lines =
coordinates.into_lines()` after `analyze_program`, then
`LocatedSourceContext::with_index(path, lines)`), keep `with_index`
(`local.rs:110-112`) as the single possession-transfer constructor, and migrate
the four test callers of the `#[cfg(test)]` `new` (`local.rs:102-108`;
`analysis/tests.rs:29,99`, `local/tests.rs:50`, `linker/graph/tests.rs:22`) to
`with_index`, building the `SourceLineIndex` from the fixture text. Guardrails:
keep `SharedSemanticArtifact` (the cache payload) strictly path-free — a cached
entry must never retain a `ProjectRelativePath`; keep `ProjectModule`
(`local.rs:466-500`) as the distinct `id` + `local` linked-module concept;
preserve `Send`/`Sync`/`Clone` behavior and the borrow-cache-then-own-record
ordering in `complete()`; `project/session/execution.rs` needs no change (it
only moves `ArtifactCacheKey`/`AnalyzedSource` in `LocalJobResult`).

**Fix Applied:** None so far.

#### [ ] READ-002 — `ArtifactCacheKey` constructor surface and `SessionState` test seams branch the cache-key path

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/local.rs:156-238` (`ArtifactCacheKey::new`/`with_engine_version`/`from_inputs` and `#[cfg(test)] for_engine_version`/`for_test_inputs` at `:212-237`), `glass-lint-core/src/project/session/mod.rs:37-48` (`SessionState` `#[cfg(test)]` fields), `:74-96` (`artifact_fingerprint` branches on those fields), `:395-403` (setters)

The cache-key path has five construction routes, and only `ArtifactCacheKey::new`
(`local.rs:157-159`) is the fixed production identity — the other four carry an
alternate engine-version or normalization-mode parameter, two of them
(`for_engine_version`, `for_test_inputs`, `local.rs:212-237`) purely to inject
alternate parameters into tests. Production `artifact_fingerprint`
(`session/mod.rs:95`) is one line, but it is wrapped in ~20 lines of
`#[cfg(test)]` branching (lines 75-94) over test-only state stored in the
production `SessionState`, modified through `ProjectSession` setters
(`:395-403`). The branching re-derives the same `(environment, limits)` pair
from the analyzer three times and keeps the test hooks in an otherwise
immutable struct (every field is fixed in `SessionState::new`; only the two
`#[cfg(test)]` fields are mutated after construction). The middle layer
`with_engine_version` (`local.rs:161-178`) is a pure nesting step whose only job
is to pick the `swc-*` normalization mode for `new` and the test wrappers.

**Recommendation:** Collapse the constructor chain to two layers. Extract the
language→mode mapping into one helper (`normalization_mode(language) -> &'static str`,
the `swc-*` strings at `local.rs:167-170`), have `ArtifactCacheKey::new`
(`local.rs:157-159`) call it with `env!("CARGO_PKG_VERSION")`, and keep
`from_inputs` (`local.rs:180-205`) as the single structural constructor with
explicit `(normalization_mode, engine_version)` parameters; delete the redundant
middle layer `with_engine_version` (`local.rs:161-178`). Collapse the two
`#[cfg(test)]` constructors into one narrow `#[cfg(test)]` key builder that
delegates to `from_inputs`. Delete the `SessionState` `#[cfg(test)]` fields and
the two setters so `artifact_fingerprint` (`session/mod.rs:74-96`) becomes
exactly `ArtifactCacheKey::new(source, self.analyzer.environment(),
self.analyzer.limits())`. The two cache_and_session tests that exercise the
engine-version and normalization dimensions (`project/tests/cache_and_session.rs:312-323`)
move to key-level assertions: construct the alternate key through the
`#[cfg(test)]` builder and assert the cache misses, the same pattern
`local/tests.rs:7-12,104-112` already uses for engine-version misses.
Guardrails: fingerprint versioning, engine-version and normalization semantics
stay frozen; the `cache_and_session.rs` contract tests
(`project/tests/cache_and_session.rs`) that drive identity reuse across
sessions must keep the same behavior through the builder; production identity
remains exactly `ArtifactCacheKey::new` with `env!("CARGO_PKG_VERSION")`.

**Fix Applied:** None so far.

### Semantic analysis boundary (`semantic/mod.rs`, `semantic/budget.rs`)

#### [x] READ-003 — `SemanticAnalyzer::name_limit` is a test-only knob stored as a production field

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/semantic/mod.rs:133-146` (field at `:136`, set to `MAX_NAMES` at `:144`), `:166` (the only production read), `:180-184` (`#[cfg(test)] with_name_limit`), `glass-lint-core/src/analysis/semantic/tests.rs:31-36,57-62`

`name_limit` is only ever non-default through the `#[cfg(test)]`
`with_name_limit` setter (`semantic/tests.rs:35,61`); in production it is always
`MAX_NAMES`, so the field is written at every construction and read at exactly
one site. The identical test seam already exists in the sibling resolver API as
an explicit parameter — `resolution::collect_with_name_limit` (`resolution/mod.rs:271-282`,
called with `MAX_NAMES` at `:267`) — which is the precedent this struct should
follow instead of a stateful builder knob. The field forces every
`SemanticAnalyzer` construction to carry a value the production path cannot
change.

**Recommendation:** Drop the field; read `MAX_NAMES` where
`NameTable::with_max_entries` is built in `analyze_program` (`semantic/mod.rs:166`)
— `with_test_collection` (`semantic/mod.rs:421`) already passes `MAX_NAMES`
directly — and give the name-exhaustion tests an explicit parameter, mirroring
`resolution::collect_with_name_limit` (e.g. a `#[cfg(test)]`
`analyze_program_with_name_limit(program, coordinates, name_limit)` that builds
the `NameTable` with the test cap), so the cap is an argument to the analysis
call, not analyzer state. Guardrails: `MAX_NAMES` and the name-exhaustion
semantics (test at `semantic/tests.rs:26-68` asserting
`semantic_name_budget_exhausted`) must produce the same artifact/status as
today; default budget/limit wiring in `analyze_source` and `with_test_collection`
is unchanged.

**Fix Applied:** Removed the production `name_limit` field and test-only
builder setter. `analyze_program` now supplies `MAX_NAMES` through a shared
explicit-limit helper, while exhaustion tests pass their cap directly; default
analysis and exhaustion diagnostics remain unchanged.

#### [x] READ-004 — `check_fact_construction_incompleteness` duplicates the budget limit that `SemanticBudget` already owns

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

**Recommendation:** Expose `SemanticBudget::limit()` (budget.rs stores `limit`
privately; the field is immutable and already read by the budget's own
`used()`/`exhausted()` logic) and narrow the check to
`check_fact_construction_incompleteness(stream, resolver)`, deriving the budget
via `resolver.budget()` and building the reason from `budget.limit()` /
`budget.used()`. Delete the `limits` and `budget` parameters and the call-site
duplication at `semantic/mod.rs:285-290`; the check is the only consumer of
`limits` in `AnalysisCompletion::assess` (`:274-294`) and `assess_completion`
(`:328-330`), so both drop the parameter too (`freeze` keeps `limits` for
`effect_operations()` at `:407`). Guardrails: the emitted reason must report
the exact same limit and used counts seen by the budget; budget construction
points (`analyze_program` at `:165`, `with_test_collection` at `:420`,
`resolution` tests) continue to derive from
`AnalysisLimits::semantic_operations()` or `UNLIMITED_SEMANTIC_OPS`.

**Fix Applied:** Added `SemanticBudget::limit()` and made completion checking
read the semantic limit directly from the resolver-owned budget. Removed the
duplicated `AnalysisLimits` plumbing from completion assessment; effect-limit
handling at the freeze boundary is unchanged.

### Status model (`semantic/status.rs`)

#### [x] READ-005 — `LocalAnalysisStatus` and `StatusScope::Local` double-encode pathless staging and force a defensive demotion bucket

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
storing raw `IncompleteReason`s in a `BTreeSet` (`IncompleteReason` already
derives `Ord`, `status.rs:38-83`, so deduplication matches `StatusEntry`'s set
semantics) instead of wrapping an `AnalysisStatus`, and have
`materialize_file(path)` produce the path-scoped `AnalysisStatus` by inserting
`StatusScope::File(path)` entries. Delete `StatusScope::Local` and the
`Local`/`Project` demotion arm in `diagnostics()` (`status.rs:183`), so
`AnalysisStatus` never contains a pathless entry and the two `record` methods
(`AnalysisStatus::record(scope, ..)` for File/Project vs.
`LocalAnalysisStatus::record(reason)`) no longer overlap. Guardrails:
materialization must stay total for the three consumers
(`linker/mod.rs:86-102`, `project/model.rs:259-277`, report assembly via
`status_snapshot` at `model.rs:421`), so the same file/project diagnostic
vectors and completion semantics hold unchanged; the test at
`status/tests.rs:55-68` keeps its assertions and records its direct `Local`
entry (`status/tests.rs:59`) through the new `LocalAnalysisStatus` API.

**Fix Applied:** Made `LocalAnalysisStatus` own a deduplicated set of raw
`IncompleteReason` values and removed `StatusScope::Local`. Materialization
now creates file-scoped entries directly; `AnalysisStatus` handles only file
and project scopes, so its diagnostic path no longer needs a local demotion
bucket. Existing local/file materialization and diagnostic ordering remain
covered by the status tests.

#### [x] READ-006 — `IncompleteReason::ParseFailure` diagnostic arm is unreachable; the skip is duplicated at the consumer

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/semantic/status.rs:173-175` (skip in `AnalysisStatus::diagnostics`), `:219-222` (dead `ParseFailure` arm in `IncompleteReason::diagnostic`), record sites `glass-lint-core/src/lint/report/mod.rs:79-88` and parser presentation `glass-lint-core/src/parse.rs:44-57`

`AnalysisStatus::diagnostics` skips `IncompleteReason::ParseFailure` before ever
calling `entry.reason.diagnostic()` (`status.rs:173-176`), so the `ParseFailure`
arm inside `IncompleteReason::diagnostic` (`status.rs:219-222`) is dead; the
classification "parse presentation is handled by the `ParseDiagnostic` owner"
is enforced twice in two places (a `matches!` in the caller plus a full
diagnostic arm that can never run). Any future direct caller of
`reason.diagnostic()` on a recorded parse failure would silently produce the
formatted diagnostic while the aggregation path goes out of its way not to.

**Recommendation:** Move the skip decision onto the reason itself so the
classification lives once. Make `diagnostic()` fallible — return
`Option<AnalysisDiagnostic>` with `ParseFailure => None` — and have
`AnalysisStatus::diagnostics` `continue` on `None`; delete the `matches!` skip
(`status.rs:173-175`) and the dead arm body (`status.rs:219-222`). This is the
minimal root-cause fix: `diagnostic()` is module-private and called from exactly
one site (`status.rs:176`), so the signature change is contained, and a future
direct caller of `reason.diagnostic()` now receives `None` instead of silently
emitting the parser-formatted diagnostic the aggregation path deliberately
avoids. A predicate-plus-total-`diagnostic()` alternative would keep a dead arm,
so prefer the `Option` form. Guardrails: the authorized
`ParseDiagnostic`/`ParseFailureKind` presentation (`parse.rs:44-57` building the
structured diagnostic, delivered via the parse-diagnostic split at
`session/artifacts.rs:196-207` and recorded for completion only at
`report/mod.rs:79-88` and `session/artifacts.rs:121-127`) remains the sole
parser reporting path, and `ParseFailure` keeps making analysis incomplete (it
is deliberately a completion side channel, per the comment at `status.rs:167-172`).

**Fix Applied:** Made `IncompleteReason::diagnostic` return `Option`, with
parse failures returning `None`, and moved the skip into that owner method.
`AnalysisStatus::diagnostics` now handles the fallible result without a
duplicated parse-failure match; parser presentation and completion semantics
remain separate.

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
  `record_local` → `from_analyzed` → `into_parts` (READ-001) is mirrored by the
  test-side ceremony: the `lower()` helper (`artifacts/tests.rs:8-14`) produces
  the same `AnalyzedSource` and the tests then call `record_analyzed`
  (`artifacts/tests.rs:30`), re-walking the identical chain. Consolidating the
  chain removes both the production layers and their test reproduction.

## Open Questions — Resolved

- **Should the line index travel with facts?** No — keep it with the
  report-facing context. The same `Arc<SourceLineIndex>` already flows from
  `parsed.lines` through `SpanNormalizer::with_index` (`semantic/mod.rs:194`),
  `LocatedSourceContext::from_normalizer` via `into_lines` (`semantic/mod.rs:96-98,198`),
  and `SharedSemanticArtifact.source_index` (`local.rs:247-252`), so the cache
  payload, the local context, and the normalizer all share one allocation. The
  index is consumed only for report-facing concerns — location conversion
  (`LocatedSourceContext::range`, `local.rs:136-138`) and request line info
  (`record_local`'s `for_each_request(..., lines, ...)`, `artifacts.rs:144-146`)
  — while `SemanticArtifact` is built by `analyze_program` from just a program
  and a `SpanNormalizer` (`semantic/mod.rs:160-178`) and is consumed directly in
  tests with no source path. Moving the index into the artifact would force the
  analysis model to retain source layout and couple facts to report concerns;
  the line index is not a path, so the merge is architecturally clean, but it is
  not required. READ-001 already reduces the extraction to one `clone_lines`
  accessor.
- **`ArtifactCacheKey` full-key retention vs. fingerprint-only identity.**
  Keep full-key retention — it is the documented fail-closed verification, not
  an accident. `CacheEntry::matches` requires fingerprint equality *and*
  full-key equality (`local.rs:272-276`), and the struct comment states the
  intent: "retaining the full key for collision verification. A fingerprint
  match is not a hit until the full key matches." (`local.rs:265-266`). The
  fingerprint is a 64-bit XXH3 over every key dimension (`local.rs:59-92`), so
  full-key equality is the safety net against a theoretical collision returning
  a wrong artifact. The cost is bounded by the FIFO-64 capacity
  (`local.rs:330`), and the retained key is path-free (`ArtifactCacheKey`
  carries `SourceText`, not a path, `local.rs:146-154`), consistent with the
  core architecture note that cached artifacts contain no path-specific source
  context. This tradeoff is already resolved by design; READ-002 does not
  change it.
- **`StatusScope::Local` removal scope.** READ-005 is viable; the premise holds.
  The only production `record(StatusScope::Local, ..)` is
  `LocalAnalysisStatus::record` (`status.rs:110-112`). Every other production
  record site uses `File` or `Project`: `linker/mod.rs:97`, `linker/graph.rs:46,64,74,85,101`,
  `linker/export.rs:62,167`, `projection/outcome.rs:54,66,82`, and
  `report/mod.rs:85,100`. The only direct `StatusScope::Local` record in tests
  is `status/tests.rs:59`, which exercises the `materialize_file` Local→File
  arm and is rewritten as part of READ-005. After the change, `AnalysisStatus`
  holds only `File`/`Project` entries and the demotion arm (`status.rs:183`)
  disappears.

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
  `AnalysisComponent`, `ResolutionKind`, `IncompleteReason` (15 variants and the
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
- `project/tests/cache_and_session.rs:65-354` — cross-session cache reuse, capacity.
- `analysis/project/linker/mod.rs:86-102` — propagates per-module
  `materialize_file` status into the project aggregate.
- `analysis/project/model.rs:259-277,421` — `single`, status snapshot.
- `analysis/project/linker/graph.rs:37-101`, `linker/export.rs:62-167`,
  `project/projection/outcome.rs:49-108` — `BudgetExhausted`/resolution/link
  status recording and `AnalysisComponent`/`ResolutionKind`/`StatusScope` use.
- `lint/report/mod.rs:79-88,111-121` — `RecordParseFailure`, status diagnostics
  and `is_complete` readiness feeding `report/summary.rs:35`.

No source, test, configuration, or dependency files were modified. The only
changes in `git status` are this audit document and the sibling
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_01..10.md` files updated by parallel
sessions, which were left untouched.
