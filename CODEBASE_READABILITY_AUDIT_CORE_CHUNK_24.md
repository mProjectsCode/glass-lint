# Codebase Readability Audit

## Summary

This audit covers Chunk 24 ("Project sessions and input types") of
`glass-lint-core`: the project input contract and validated newtypes, the
staged error boundary, resolution requests and typed resolver outcomes, the
project-session coordinate layer, the local-analysis artifact and execution
modules, the source/resolution tables, and report combination. It is
read-only; no source was modified. Only
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_24.md` was created; the pre-existing
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_*.md` files from parallel sessions were
left untouched. Chunk 25 ("Project report types") already exists and covers
the `project/types/report/**` family plus `project/report/mod.rs`; this audit
does not re-report those, and `ReportCombineError::combine` is only referenced
for ownership boundaries.

The chunk is largely well-built: `ProjectRelativePath`, `SourceText`,
`PackageSpecifier`, `BuiltinModuleName`, `NormalizedOutsidePath`, and
`ModuleId` are validated/private-storage newtypes with narrow domain accessors;
`ProjectSession` exposes a clean three-move lifecycle
(`analyze_source`/`analyze_sources` → `finish`) that validates authored
resolution outcomes before linking, so unknown, duplicate, or missing
resolutions fail closed; `SourceTable::admit_all`, `AnalysisArtifacts::validate_complete`,
and `AuthoredRequests` ownership keep analysis bounded and deterministic; and the
`ModuleInterface → ResolutionRequestKey/ResolutionRequest` boundary is coherent
(role→kind mapping lives once in `analysis/model/module.rs:149-156`, and
`for_each_request` at `module.rs:352-370` keys each occurrence by importer, kind,
and line/column range).

The findings concentrate on five seams: a single failure mode smeared across
three nested error enums; Node `node:` builtin policy baked into
provider-neutral core; test-only telemetry driving the production
`ExecutionEvent` dispatch (including a notifier helper that exists only for
that telemetry); the in-flight bound formula duplicated across the two bounded
executors; and a `pub` module whose entire surface is `pub(crate)`.

## Findings

### Staged error boundary

#### [ ] READ-001 — One failure mode is smeared across three nested error enums

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/input/errors.rs:5-9,45-48,51-56,58-74,124-130`; constructions `glass-lint-core/src/project/session/mod.rs:349`, `glass-lint-core/src/lint/batch.rs:129-133`; consumers `glass-lint-project/src/error.rs:46,124,148,154-161`, `glass-lint-project/src/tests.rs:79-81`

`LocalExecutionError` has exactly one variant (`WorkerPanic`), and
`ProjectExecutionError` wraps exactly that enum under `Local`, so the only
execution failure ever produced is
`ProjectError::Execution(ProjectExecutionError::Local(LocalExecutionError::WorkerPanic))`
— three nesting levels for a single bit of information. The only construction
sites confirm this: `session/mod.rs:349` maps a `LocalExecutionError` from the
executor, and `batch.rs:129-133` (`worker_panic()`) builds the same tree; every
other producer returns `LocalExecutionError::WorkerPanic` directly
(`execution.rs:212,276,288`) and never uses the outer wrappers. Both wrappers
are re-exported at the `project` facade (`project/mod.rs:20-21`) and the
`types` facade (`types/mod.rs:13-14`), multiply the `Display`/`source`
plumbing (`errors.rs:11-17,124-130,132-152`), and force every downstream
consumer (`ProjectLoadError::from(ProjectError)` at `error.rs:154-161`, plus
the `Execution` Display at `error.rs:124` and `source()` at `error.rs:148`) to
destructure or re-wrap a tree that can hold one value. The claimed "staged"
boundary does stage input vs phase vs execution, but `Execution` does not earn
two extra enums for a single `WorkerPanic` discriminant.

**Recommendation:** Delete `ProjectExecutionError` and give `ProjectError` a
third variant that holds the failure directly, e.g.
`ProjectError::Execution(LocalExecutionError)` (or, if the two names must both
survive, fold `LocalExecutionError` into `ProjectExecutionError` and drop one
level). Update the two construction sites (`session/mod.rs:349` already maps a
`LocalExecutionError`; `batch.rs:129-133 worker_panic()`), the loader's
`Execution` variant and its `Display`/`source()`/`From` impls in
`glass-lint-project/src/error.rs:46,124,148,154-161`, the affected test
(`glass-lint-project/src/tests.rs:79-81`; core's `lint/batch/tests.rs` uses the
`worker_panic()` helper, so it is unaffected), and the re-export lists at
`project/mod.rs:20-21` and `types/mod.rs:13-14`. Guardrails: preserve the
`Input`/`Phase`/`Execution` three-stage split that `ProjectLoadError` maps
separately, keep the `WorkerPanic` meaning distinct from `Phase`, and keep the
current effective `Display` text and `std::error::Error::source()` output
stable for CLI users (the loader's "core project execution failed: …" prefix
and the "local analysis execution failed: analysis worker panicked" chain must
still render).

**Fix Applied:** None so far.

### Provider-neutral input contract

#### [ ] READ-002 — `BuiltinModuleName` hardcodes Node `node:` policy in provider-neutral core

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/types/input.rs:131-156` (`node:` check at 143-145); construction site `glass-lint-project/src/resolver.rs:82-86`; re-export and consumers `glass-lint-core/src/project/types/mod.rs:12-17` and `glass-lint-harness/src/types/protocol.rs:126`

`BuiltinModuleName::new` rejects every name that is not a literal `node:` prefix
(`input.rs:143-145`), which bakes a Node.js host convention into
`glass-lint-core`. The workspace contract (AGENTS.md; core `ARCHITECTURE.md`
lines 83-84: "Core contains no provider names, APIs, profiles, categories, or
manifests"; `glass-lint-js` owns JavaScript, browser, Node.js, and Electron
policy) places host/runtime policy outside core. The "is this request a
builtin" decision is already made by the resolver in `glass-lint-project`
(`resolver.rs:82`, from oxc's `ResolveError::Builtin`, configured with
`builtin_modules: true` and `node` condition names at `resolver.rs:40,52-56`),
so core only needs a validated opaque builtin name, not a whitelist of one host
scheme. Encoded host policy also cannot be extended for other environments
without touching core, and the case-variant rejections (`Node:fs`, `NODE:fs`,
`tests.rs:130-131`) are pure Node policy.

**Recommendation:** Move the `node:` recognition to the owner of host
resolution policy (`glass-lint-project` resolver, or `glass-lint-js` if a
policy crate must decide), and shrink `BuiltinModuleName`'s validation to the
provider-neutral invariants — a non-empty `scheme:name` shape with both parts
non-empty and no whitespace or NUL (which still rejects `fs`, `node:`,
`node: fs`, and empty names, and keeps `node:fs` validating, without judging
which scheme is a real host). This is the primary option. The conservative
alternative is to keep the strict newtype but construct it from the
resolver-provided, already-classified value (`resolver.rs:82-86`) rather than
re-validating the scheme inside core. Update the `node:`-specific
positives/negatives in `glass-lint-core/src/project/types/input/tests.rs:95-153`
(the case-variant rejections at 127-132 become provider-policy tests or drop)
and the harness expectation builder (`protocol.rs:124-128`). Guardrails:
preserve fail-closed rejection of malformed names, keep the serialized `node:fs`
spelling stable for reports and cases (the stored value is the resolver's
`resolved` string, so `as_str()` output is unchanged), and keep
`BuiltinModuleName` as the validated storage type so
`ResolvedTargetKind::Builtin` remains unusable with arbitrary strings.

**Fix Applied:** None so far.

### Session execution coordinates

#### [ ] READ-003 — Test-only execution telemetry drives the production `ExecutionEvent` dispatch

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/project/session/execution.rs:60-81,249-291`; observation seams `glass-lint-core/src/project/session/mod.rs:118-185`, `glass-lint-core/src/project/session/artifacts.rs:216-227`; the only non-test consumer is `NoopExecutionObserver` at `execution.rs:78-81`

`ExecutionEvent` carries ten variants (`Submitted`, `Started`, `Finished`,
`Merged`, `ParseAttempted`, `AnalysisAttempted`, `CacheHit`, `CacheMiss`,
`CacheInserted`, `CacheEvicted`), and every production execution path — the
parallel loop (`execution.rs:249-291`), the transition callbacks
(`session/mod.rs:118-185`), and the cache insert helper
(`artifacts.rs:216-227`) — dispatches them into a `&dyn ExecutionObserver`
that in production is always `NoopExecutionObserver`, which discards every
event. The observer abstraction is a legitimate way to make the deterministic
concurrency/cache tests observable, but the seam placement makes `observe`
a no-op call at every production boundary. Notably `insert_and_notify`
(`artifacts.rs:216-227`) exists solely to pair a normal cache insert with
`CacheInserted`/`CacheEvicted` events that only the `#[cfg(test)]`
`CountingExecutionObserver` (`execution.rs:83-175`) reads; without those two
events the helper is a bare `cache.insert_analyzed(key, analyzed)`.

**Recommendation:** Collapse the event stream to the four ordered transitions
the concurrency-peak tests actually need — `Submitted`, `Started`, `Finished`,
`Merged` — and drop the six totals-only counting events
(`ParseAttempted`/`AnalysisAttempted`/`CacheHit`/`CacheMiss`/`CacheInserted`/
`CacheEvicted`). The peaks are measured as the window evolves across the
executor loop and the callbacks, so they require event order; the count
assertions are totals-only and can be derived from thin `#[cfg(test)]` counted
wrappers around `SemanticAnalyzer::analyze_source` and around cache get/insert.
Delete `insert_and_notify` and fold the cache insert back into
`LocalAnalysisTransition::complete` (`session/mod.rs:147-163`). Guardrails:
keep the peak-active/peak-outstanding bounds asserted in
`project/tests/mod.rs:185-204` (`active <= workers`,
`outstanding <= outstanding_job_bound`) and the cache-hit/analysis-count
invariants in
`project/tests/cache_and_session.rs:151-181` (hits == 1, analyses == 0 on
reuse); the panic-discard path must still balance outstanding accounting (the
`discard` callback observes `Merged` at `session/mod.rs:183`) so a worker panic
surfaces exactly once as a `WorkerPanic` execution error.

**Fix Applied:** None so far.

#### [ ] READ-004 — The in-flight bound formula `workers × 2` is duplicated across the two bounded executors

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/session/execution.rs:357-359` (`outstanding_job_bound`), `glass-lint-core/src/lint/batch.rs:36-42` (`BatchOptions::from_workers`), asserted in `glass-lint-core/src/project/tests/mod.rs:185-204`

`outstanding_job_bound(worker_limit) = worker_limit.get().saturating_mul(2)` and
`BatchOptions::from_workers`'s `max_in_flight = workers.saturating_mul(2).max(1)`
encode the same invariant — bounded in-flight window of twice the worker count
— once in the project-session executor and once in the batch linter. The two
executors are genuinely separate layers (an independent one-file project per
batch vs. one session with a shared artifact cache), so the formula is the only
shared part, but it is stated twice and their tests assert the same bound with
the same arithmetic; a change to one window policy would silently leave the
other behind.

**Recommendation:** Introduce one crate-internal helper (e.g., an
`in_flight_window(worker_count: usize) -> usize` in a shared bounds module)
implementing `saturating_mul(2).max(1)`, and have both
`ThreadLocalJobExecutor::execute` (`execution.rs:232`) and
`BatchOptions::from_workers` (`batch.rs:37`) call it. The `max(1)` is a no-op
for the session path (its input is `NonZeroUsize`, already ≥ 1) and preserves
the batch's over-0 semantics, so both call sites keep identical arithmetic —
including `in_flight_window(usize::MAX) == usize::MAX` since
`saturating_mul(2).max(1)` of `usize::MAX` saturates to `usize::MAX`. Guardrails:
keep the two executors independent (do not merge `LintBatch` drivers and
session waves); preserve the `outstanding_job_bound(usize::MAX) == usize::MAX`
assertion (`project/tests/mod.rs:200-203`) and the batch `max_in_flight`
vector-slot behavior in `lint/batch.rs` (including the `with_max_in_flight`
override at `batch.rs:46-49`).

**Fix Applied:** None so far.

### Module ownership advertisement

#### [ ] READ-005 — `pub mod input` advertises a module whose entire surface is `pub(crate)`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/mod.rs:8`; `glass-lint-core/src/project/input.rs:26,50` (`pub(crate) fn normalize_relative`, `normalize_outside_target`)

`project/mod.rs` declares `pub mod input`, but the module's only contents are
two `pub(crate)` normalization helpers, and its only direct callers are the two
`types` constructors: `normalize_relative` is called by
`ProjectRelativePath::new` (`types/mod.rs:63`) and `normalize_outside_target` by
`NormalizedOutsidePath::new` (`types/input.rs:165`). The public module boundary
therefore advertises an empty public API in docs, every item is effectively
private, and the `pub` marking presents the internal normalization surface as
if it were part of the project contract. (The module's own doc comment at
`input.rs:1-5` claims the helpers are "used by the session, types, and CLI
loading code"; in fact the session and loading code only reach them
indirectly, through the two `types` constructors, and the helpers are
`pub(crate)` so no other crate can call them.) Sibling modules of `project`
(`report`, `session`, `tables`) are correctly declared private with selective
re-exports.

**Recommendation:** Change `pub mod input;` to `mod input;` so the module is a
private implementation module, matching `report`/`session`/`tables`. Guards:
keep both helpers `pub(crate)` unchanged — `types/input.rs:165` and
`types/mod.rs:63` are the only direct callers, and the helpers are crate-private
so there are no downstream users of `project::input` to update. No other crate
is affected.

**Fix Applied:** None so far.

## Systemic Themes

- **Executors are built twice to a common shape.** `lint/batch.rs`
  (`PendingBatch`, `CompletedBatch`, `BatchResult`, `worker_panic()`,
  `catch_unwind` + Rayon pool) and `project/session/execution.rs`
  (`WorkerPool`/`ThreadLocalJobExecutor`, `LocalJobResult`, panic catch,
  same window formula) implement the same bounded, panicking,
  ordering-preserving execution pattern at two layers. The layers are
  legitimately different (independent one-file projects vs. one shared-cache
  project), but READ-003 and READ-004 are the narrowest shared seams; a future
  unification should stop at sharing the window bound and any measurement
  scaffolding, not the job lifecycles.
- **Consistent validate-at-construction newtypes, one deferred check.**
  `ProjectRelativePath`, `PackageSpecifier`, `BuiltinModuleName`,
  `NormalizedOutsidePath`, and `SourceRange` all reject malformed input at
  construction (`types/mod.rs:62`, `types/input.rs:98,137,163`,
  `glass-lint-datastructures/src/diagnostic.rs:187`); `ResolverOutcome::validate`
  (`resolution.rs:102-109`) is the exception, applied only inside
  `AnalysisArtifacts::into_link_input` (`artifacts.rs:189`) after session work
  has run. The production resolver builds every `Unsupported { reason }` from a
   `format!` prefix that is never empty
   (`resolver.rs:99-100,122-123,130-131,144-145`), so the check is a fail-closed
   safety net rather than a routinely exercised path — but it is genuinely
   reachable from the authored/adapter side, where the reason is
   caller-supplied (`protocol.rs:139-142`, `cases/project.rs:59`), so it is a
   live defense, not dead code.
- **Error staging is real at the top and padded below.** The
  `Input`/`Phase`/`Execution` split maps cleanly onto `ProjectLoadError`, but
  READ-001 shows the `Execution` leg is over-modeled, and
  `ProjectInputError::InvalidPath`/`InvalidTarget` and
  `ProjectPhaseError::InvalidTarget` overlap in message while staying
  stage-separated.

## Open Questions — Resolved

1. **The telemetry is worth keeping as ordered events for the peaks, but not
   as ten variants for the counts.** The peak-active/peak-outstanding
   assertions (`project/tests/mod.rs:197-198`) measure the concurrency window
   as it evolves across the executor loop and the transition callbacks
   (`Submitted` increments outstanding, `Started`/`Finished` drive active,
   `Merged` balances outstanding at release and on the panic-discard path,
   `session/mod.rs:179,183`), so those four events genuinely need their order;
   a counted wrapper around `analyze_source` alone cannot see the
   submit/release window. The remaining six events (`ParseAttempted`,
   `AnalysisAttempted`, `CacheHit`, `CacheMiss`, `CacheInserted`,
   `CacheEvicted`) are asserted only as totals
   (`cache_and_session.rs:50-60,110-113,345-353`), so thin `#[cfg(test)]`
   wrappers around `analyze_source` and cache get/insert can replace them. This
   resolves READ-003 in favor of the guardrail's "latter" assumption, and the
   collapsed set is exactly `Submitted`/`Started`/`Finished`/`Merged`.
2. **`ResolverOutcome::validate` is a deliberate fail-closed defense and it is
   reachable — not dead.** The production resolver never emits an empty reason
   (`resolver.rs:99-100,122-123,130-131,144-145` all use non-empty `format!`
   prefixes), but the authored/adapter path passes a caller-supplied reason
   through unchanged (`protocol.rs:139-142`, sourced from the manifest's
   `Unsupported { reason }` at `cases/project.rs:59,105-106`), and the harness
   drives `session.finish(outcomes)` (`adapters.rs:169`), so a manifest with
   `reason = ""` reaches `validate()` and is rejected with
   `ProjectPhaseError::InvalidTarget("")`. The check should stay. Moving it
   into a fallible `unsupported(reason)` constructor would make the invalid
   state unrepresentable, but that means touching all five production
   construction sites (`resolver.rs:99,122,130,144` and `protocol.rs:140`) plus
   tests, so keeping `validate` as the single enforcement point at the
   authored-outcome boundary is the minimal fix. `ProjectPhaseError::InvalidTarget`
   is the right error for a malformed resolver-answer target; no
   rename is warranted.
3. **Both wrappers earn their keep.** `AuthoredRequests` is the public return
   type of the session's `analyze_source`/`analyze_sources` methods
   (`session/mod.rs:221,310`, re-exported at `project/mod.rs:15`), and its
   `len`/`iter`/`is_empty`/`IntoIterator` surface is actually consumed by the
   loader (`loader.rs:289` `requests.len()`, `loader.rs:365`
   `for request in requests`) and by core tests
   (`session_and_link_validation.rs:37,50-54,100`, `support.rs:204`); the
   vocabulary distinguishes "requests authored by
   completed analysis" from raw resolution input at the crate boundary.
   `ResolutionTable` is indeed constructed and drained entirely inside
   `into_link_input` (`artifacts.rs:184-191` → `model.rs:176-185`), but its
   `insert` enforces the duplicate-resolution rejection
   (`tables.rs:125-135`, surfaced as `ProjectPhaseError::DuplicateResolution`),
   which a bare `BTreeMap` insert would silently lose. Retention is confirmed;
   no change is proposed.
4. **The core cap is the enforcement point; the loader's identical read is
   redundant, and the batch cap is a separate layer.** `normalize_worker_limit`
   (`execution.rs:350-355`) is the single cap for the session executor and is
   genuinely needed as a safety net for direct core callers — `analyze_sources`
   is public and accepts an arbitrary `NonZeroUsize` (`session/mod.rs:306-315`).
   The loader's `available_parallelism()` read (`loader.rs:238`) feeds the same
   `analyze_sources` path, where core re-caps to the identical value, so that
   read is duplicated work but behaviorally harmless (the cap is idempotent);
   the loader could pass its requested worker count straight through. By
   contrast `BatchOptions::new`/`Default` (`batch.rs:32-34,60-64`) legitimately
   needs its own host cap because the batch executor never routes through the
   session's `normalize_worker_limit`. Not a defect; at most a one-line
   simplification at `loader.rs:238`.
5. **The range-keyed occurrence and the semantic specifier cache are layered,
   and they agree because resolution is range-independent.** `ResolutionCache`
   checks the occurrence key (which includes the range) first, then the
   semantic `by_specifier` key (importer + kind + specifier) at
   `loader_phases.rs:61-85`; repeated imports of the same specifier at
   different ranges share one `by_specifier` outcome but keep distinct
   occurrence entries with their own ranges. The two agree because the resolver
   outcome depends only on (importer, kind, specifier) — the range never
   reaches the resolver. Sources are immutable within a session (`SourceTable`
   only grows via `admit_all`), so the range is a stable occurrence identity.
   A future re-analysis of edited text would invalidate both the occurrence
   ranges and the specifier-keyed reuse policy, so it is indeed a semantics
   question, not a readability one.

## Coverage

Files read in full and cited in findings:

- `glass-lint-core/src/project/mod.rs`, `glass-lint-core/src/project/input.rs`
- `glass-lint-core/src/project/report/mod.rs` (ownership boundary only; the
  report-value family belongs to Chunk 25)
- `glass-lint-core/src/project/session/mod.rs` (+ `execution.rs`,
  `artifacts.rs`)
- `glass-lint-core/src/project/session/artifacts/tests.rs`
- `glass-lint-core/src/project/tables.rs`
- `glass-lint-core/src/project/types/mod.rs`, `types/input.rs`,
  `types/input/errors.rs`, `types/input/resolution.rs`,
  `types/input/tests.rs`
- `glass-lint-core/src/project/tests/{mod,input_validation,cache_and_session,
  session_and_link_validation,linking_and_flow,support,status_policy}.rs`
- `glass-lint-core/src/lint/linter.rs`, `glass-lint-core/src/lint/batch.rs`
  (+ `batch/tests.rs`), `glass-lint-core/src/lint/report/mod.rs`
- `glass-lint-core/src/analysis/model/module.rs`,
  `glass-lint-core/src/analysis/project/model.rs`,
  `glass-lint-core/src/analysis/project/resolver.rs`
- `glass-lint-project/src/loader.rs`, `loader_phases.rs`, `resolver.rs`,
  `error.rs`, `tests.rs`
- `glass-lint-harness/src/types/protocol.rs`,
  `glass-lint-harness/src/adapters.rs`, `glass-lint-harness/src/cases/project.rs`
- `glass-lint-datastructures/src/diagnostic.rs` (`SourceRange`)
- Architecture and guidance: `AGENTS.md`, `ARCHITECTURE.md`,
  `glass-lint-core/ARCHITECTURE.md`, `CODEBASE_STRUCTURE_CORE.md` (Chunk 24
  listing)

No source, tests, configuration, or documentation were modified; the only file
this audit writes is `CODEBASE_READABILITY_AUDIT_CORE_CHUNK_24.md`. The
pre-existing `CODEBASE_READABILITY_AUDIT_CORE_CHUNK_*.md` files were left
untouched.