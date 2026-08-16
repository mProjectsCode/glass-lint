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
resolution outcomes before linking, so unambiguity and missing resolutions
fail close; `SourceTable::admit_all`, `AnalysisArtifacts::validate_complete`,
and `AuthoredRequests` ownership keep analysis bounded and deterministic; and
the `ModuleInterface → ResolutionRequestKey/ResolutionRequest` boundary is
coherent (role→kind mapping lives once in
`analysis/model/module.rs:149-156`, recognized by the keyed occurrence model).

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
- **Location:** `glass-lint-core/src/project/types/input/errors.rs:3-9,45-48,51-56,58-74,124-130`; constructions `glass-lint-core/src/project/session/mod.rs:349`, `glass-lint-core/src/lint/batch.rs:129-133`; consumers `glass-lint-project/src/error.rs:46,154-160`, `glass-lint-project/src/tests.rs:79-80`

`LocalExecutionError` has exactly one variant (`WorkerPanic`), and
`ProjectExecutionError` wraps exactly that enum under `Local`, so the only
execution failure ever produced is
`ProjectError::Execution(ProjectExecutionError::Local(LocalExecutionError::WorkerPanic))`
— three nesting levels for a single bit of information. Both wrappers are
re-exported at the `project` facade (`project/mod.rs:21`), multiply the
`Display`/`source` plumbing, and force every downstream consumer
(`ProjectLoadError::from(ProjectError)` at `error.rs:154-160`) to destructure
or re-wrap a tree that can hold one value. The claimed "staged" boundary does
stage input vs phase vs execution, but `Execution` does not earn two extra
enums for a single `WorkerPanic` discriminant.

**Recommendation:** Delete `ProjectExecutionError` and give `ProjectError` a
third variant that holds the failure directly, e.g.
`ProjectError::Execution(LocalExecutionError)` (or, if the two names must both
survive, fold `LocalExecutionError` into `ProjectExecutionError` and drop one
level). Update the two construction sites (`session/mod.rs:349` already maps a
`LocalExecutionError`; `batch.rs:129-133 worker_panic()`), the loader's `From`
impl in `glass-lint-project/src/error.rs`, and the affected tests in both
crates. Guardrails: preserve the `Input`/`Phase`/`Execution` three-stage
split that `ProjectLoadError` maps separately, keep the `WorkerPanic` meaning
distinct from `Phase`, and keep the current `Display` text and
`std::error::Error::source()` output stable for CLI users.

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
"Core contains no provider names"; `glass-lint-js` owns JavaScript, browser,
Node.js, and Electron policy) places host/runtime policy outside core. The
"is this request a builtin" decision is already made by the resolver in
`glass-lint-project` (`resolver.rs:82`, from oxc's `ResolveError::Builtin`),
so core only needs a validated opaque builtin name (non-empty, no
whitespace/NUL), not a whitelist of one host scheme. Encoded host policy also
cannot be extended for other environments without touching core.

**Recommendation:** Move the `node:` recognition to the owner of host
resolution policy (`glass-lint-project` resolver, or `glass-lint-js` if a
policy crate must decide), and shrink `BuiltinModuleName`'s validation to the
provider-neutral invariants (single non-empty scheme segment, no whitespace or
NUL) — or keep the strict newtype but construct it from a resolver-provided,
already-classified value rather than re-validating the scheme. Update the
`node:`-specific positives/negatives in
`glass-lint-core/src/project/types/input/tests.rs:95-152` and the harness
protocol expectation builder. Guardrails: preserve fail-closed rejection of
malformed names, keep the serialized `node:fs` spelling stable for reports and
cases, and keep `BuiltinModuleName` as the validated storage type so
`ResolvedTargetKind::Builtin` remains unusable with arbitrary strings.

**Fix Applied:** None so far.

### Session execution coordinates

#### [ ] READ-003 — Test-only execution telemetry drives the production `ExecutionEvent` dispatch

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/project/session/execution.rs:60-76,78-81,249-276`; observation seams `glass-lint-core/src/project/session/mod.rs:118-184`, `glass-lint-core/src/project/session/artifacts.rs:216-227`; the only non-test consumer is `NoopExecutionObserver` at `execution.rs:78-81`

`ExecutionEvent` carries ten variants (`Submitted`, `Started`, `Finished`,
`Merged`, `ParseAttempted`, `AnalysisAttempted`, `CacheHit`, `CacheMiss`,
`CacheInserted`, `CacheEvicted`), and every production execution path — the
parallel loop (`execution.rs:249-276`), the transition callbacks
(`session/mod.rs:118-184`), and the cache insert helper
(`artifacts.rs:216-227`) — dispatches them into a `&dyn ExecutionObserver`
that in production is always `NoopExecutionObserver`, which discards every
event. The observer abstraction is a legitimate way to make the deterministic
concurrency/cache tests observable, but the seam placement makes `observe`
a no-op call at every production boundary. Notably `insert_and_notify`
(`artifacts.rs:216-227`) exists solely to pair a normal cache insert with
`CacheInserted`/`CacheEvicted` events that only the `#[cfg(test)]`
`CountingExecutionObserver` (`execution.rs:83-175`) reads.

**Recommendation:** Collapse the event stream to the transitions that cross a
module seam and are actually asserted — admit/submit, finished, merged,
discarded — and derive parse/analysis/cache counts from the operations
themselves (e.g., a thin counted wrapper around `SemanticAnalyzer#analyze_source`
and around cache get/insert, used only in tests). Delete `insert_and_notify`
and fold the cache insert back into `LocalAnalysisTransition::complete`
(`session/mod.rs:147-163`). Guardrails: keep the peak-active/peak-outstanding
bounds asserted in
`project/tests/mod.rs:186-204` (`active <= workers`, `outstanding <=
outstanding_job_bound`) and the cache-hit/analysis-count invariants in
`project/tests/cache_and_session.rs:151-181` (hits == 1, analyses == 0 on
reuse); the panic-discard path must still balance outstanding accounting so a
worker panic surfaces exactly once as a `WorkerPanic` execution error.

**Fix Applied:** None so far.

#### [ ] READ-004 — The in-flight bound formula `workers × 2` is duplicated across the two bounded executors

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/session/execution.rs:357-359` (`outstanding_job_bound`), `glass-lint-core/src/lint/batch.rs:36-42` (`BatchOptions::from_workers`), asserted in `glass-lint-core/src/project/tests/mod.rs:195-203`

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
`in_flight_window(worker_count) -> usize` in a shared bounds module) and have
both `ThreadLocalJobExecutor::execute` and `BatchOptions::from_workers` call
it, keeping the `max(1)` over-0 semantics. Guardrails: keep the two executors
independent (do not merge `LintBatch` drivers and session waves); preserve the
`outstanding_job_bound(usize::MAX) == usize::MAX` assertion
(`project/tests/mod.rs:200-203`) and the batch `max_in_flight` vector-slot
behavior in `lint/batch.rs`.

**Fix Applied:** None so far.

### Module ownership advertisement

#### [ ] READ-005 — `pub mod input` advertises a module whose entire surface is `pub(crate)`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/mod.rs:8`; `glass-lint-core/src/project/input.rs:26,50` (`pub(crate) fn normalize_relative`, `normalize_outside_target`)

`project/mod.rs` declares `pub mod input`, but the module's only contents are
two `pub(crate)` normalization helpers used by `project/types` constructors
(`ProjectRelativePath::new`, `NormalizedOutsidePath::new`) and by session and
CLI loading code. The public module boundary therefore advertises an empty
public API in docs, every item is effectively private, and the `pub` marking
presents the internal normalization surface as if it were part of the project
contract. Sibling modules of `project` (`report`, `session`, `tables`) are
correctly declared private with selective re-exports.

**Recommendation:** Change `pub mod input;` to `mod input;` so the module is a
private implementation module, matching `report`/`session`/`tables`. Guards:
keep both helpers `pub(crate)` unchanged — `types/input.rs`, `project/types/mod.rs`
and the session are the only callers; there are no downstream users of
`project::input` to update.

**Fix Applied:** None so far.

## Systemic Themes

- **Executors are built twice to a common shape.** `lint/batch.rs`
  (`PendingBatch`, `CompletedBatch`, `BatchResult`, `worker_panic()`,
  `catch_unwind` + Rayon pool) and `project/session/execution.rs`
  (`WorkPool`/`ThreadLocalJobExecutor`, `LocalJobResult`, panic catch,
  same window formula) implement the same bounded, panicking,
  ordering-preserving execution pattern at two layers. The layers are
  legitimately different (independent one-file projects vs. one shared-cache
  project), but READ-003 and READ-004 are the narrowest shared seams; a future
  unification should stop at sharing the window bound and any measurement
  scaffolding, not the job lifecycles.
- **Consistent validate-at-construction newtypes, one deferred check.**
  `ProjectRelativePath`, `PackageSpecifier`, `BuiltinModuleName`,
  `NormalizedOutsidePath`, and `SourceRange` all reject malformed input at
  construction; `ResolverOutcome::validate` (`resolution.rs:102-109`) is the
  exception, applied only inside `AnalysisArtifacts::into_link_input`
  (`artifacts.rs:189`) after session work has run. Today every resolved
  `Unsupported { reason }` is built from a `format!` that is never empty, so
  the check is defensive rather than reachable; see Open Questions.
- **Error staging is real at the top and padded below.** The
  `Input`/`Phase`/`Execution` split maps cleanly onto `ProjectLoadError`, but
  READ-001 shows the `Execution` leg is over-modeled, and
  `ProjectInputError::InvalidPath`/`InvalidTarget` and
  `ProjectPhaseError::InvalidTarget` overlap in message while staying
  stage-separated.

## Open Questions

- **Is the telemetry (READ-003) worth keeping as counts at all?** The
  concurrency-bound and cache-reuse tests depend on observing peaks and cache
  hits/misses. If the counts are only asserted by `#[cfg(test)]` observers,
  would test-only counting wrappers around `analyze_source` and the cache be
  simpler than the ten-event enum, or does the merged/outstanding accounting
  need the event order? The guardrails above assume the latter.
- **`ResolverOutcome::validate` empty-reason rejection is currently
  unreachable** because every `ResolvedTargetKind::Unsupported` reason is
  produced by a non-empty `format!` in `glass-lint-project/src/resolver.rs`. Is
  this deliberate fail-closed defense, or should the check move to a fallible
  `unsupported` constructor so the invalid state is unrepresentable? Should
  `ProjectPhaseError::InvalidTarget` be reused for this or renamed to
  communicate the resolver-answer origin?
- **Are the `AuthoredRequests` and `ResolutionTable` wrappers worth their
  keep as separate types?** `AuthoredRequests` (artifacts.rs:68-105) forwards
  `iter`/`len`/`IntoIterator` and is consumed immediately by the loader;
  `ResolutionTable` (tables.rs:119-145) is constructed and drained entirely
  inside `into_link_input`. They do carry phase vocabulary and a
  duplicate-rejection invariant respectively, which argues for retention; no
  change is proposed until a caller demonstrates the vocabulary is not needed.
- **`normalize_worker_limit` (`execution.rs:350-355`) caps to host
  parallelism inside core while `glass-lint-project/src/loader.rs:238` and
  `BatchOptions::new` compute the same host cap at the boundary.** Is the
  double capping a real policy duplication across crates, or a deliberate
  safety net for direct core callers?
- **Does `ResolutionRequestKey`'s occurrence-level `SourceRange`
  (line/column) key interact with the loader's semantic `by_specifier` cache
  (`glass-lint-project/src/loader_phases.rs:34-89`)?** The two keys agree today
  because sources are immutable within a session; a future re-analysis of
  edited text would need the key and the cache invalidation policy revisited,
  which is a semantics question, not a readability one.

## Coverage

Inspected (read-only) source:

- `glass-lint-core/src/project/mod.rs`, `glass-lint-core/src/project/input.rs`
- `glass-lint-core/src/project/report/mod.rs` (ownership boundary only; the
  report-value family belongs to Chunk 25)
- `glass-lint-core/src/project/session/mod.rs` (+ `execution.rs`,
  `artifacts.rs`)
- `glass-lint-core/src/project/session/artifacts/tests.rs`
- `glass-lint-core/src/project/tables.rs`
- `glass-lint-core/src/project/types/mod.rs`, `types/input.rs`,
  `types/input/errors.rs`, `types/input/resolution.rs`
- `glass-lint-core/src/project/tests/{mod,input_validation,cache_and_session,
  session_and_link_validation,linking_and_flow,support}.rs`
- `glass-lint-core/src/lint/linter.rs`, `glass-lint-core/src/lint/batch.rs`,
  `glass-lint-core/src/lint/report/mod.rs`
- `glass-lint-core/src/analysis/model/module.rs`,
  `glass-lint-core/src/analysis/project/model.rs`,
  `glass-lint-core/src/analysis/project/resolver.rs`
- `glass-lint-project/src/loader.rs`, `loader_phases.rs`, `resolver.rs`,
  `error.rs`, `tests.rs`
- `glass-lint-harness/src/types/protocol.rs`
- `glass-lint-datastructures/src/diagnostic.rs` (`SourceRange`)
- Architecture and guidance: `AGENTS.md`, `ARCHITECTURE.md`,
  `glass-lint-core/ARCHITECTURE.md`, `CODEBASE_STRUCTURE_CORE.md` (Chunk 24
  listing)

No source, tests, configuration, or documentation were modified; the only file
created is `CODEBASE_READABILITY_AUDIT_CORE_CHUNK_24.md`. The pre-existing
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_*.md` files were left untouched.