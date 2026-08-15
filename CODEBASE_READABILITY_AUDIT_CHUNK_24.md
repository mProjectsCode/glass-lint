# Codebase Readability Audit — glass-lint-core Chunk 24: Project sessions and input types

## Summary

Chunk 24 owns the project session API (`ProjectSession`, `SessionState`,
`LocalAnalysisTransition`), the local-analysis artifact/execution layers
(`AnalysisArtifacts`, `AuthoredRequests`, `ThreadLocalJobExecutor`), the
internal tables (`SourceTable`, `ResolutionTable`, `AuthoredRequestTable`),
the validated input types (`SourceFile`, `SourceText`, `ProjectRelativePath`,
`PackageSpecifier`, `BuiltinModuleName`, `NormalizedOutsidePath`,
`ResolutionRequest{Key}`, `ResolverOutcome`, `ModuleId`), the staged error
boundary (`ProjectInputError`, `ProjectPhaseError`, `ProjectExecutionError`,
`LocalExecutionError`, `ProjectError`), and the cross-chunk `AnalysisReport::combine`
entry point in `project::report`.

The architecture is generally coherent: ownership is clear
(`SessionState` bundles `Linter`-owned borrows; `AnalysisArtifacts` keeps its
maps private; `AuthoredRequestTable` is a real domain collection; the error
enum hierarchy is staged by phase). The strongest issues are API-surface debt —
public accessors with no production callers (`SourceFile::from_relative`,
`SourceFile::into_path`, `SourceFile::into_source`,
`ResolutionRequest::range_owned`) — plus a public `SourceTable::insert` that
duplicates `admit_all` with fabricated limit values and is exercised only via
`admit_all`'s internal staging and tests. Secondary issues are parallel enum
vocabulary
(`ResolverOutcome`/`LinkedModuleTarget`), a sort that re-implements the key's
derived `Ord`, a garbled module doc comment, an overloaded `InvalidPath` error
used for missing-source lookups, a `cfg(test)` fingerprint seam duplicating
production logic, and small test-side duplication.

No source, test, or configuration files were changed.

## Findings

### Project input types (`project/types/input.rs`, `project/types/input/resolution.rs`)

#### [x] READ-001 — Dead and test-only accessors on public project input types

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/input.rs:214-216, 248-254`; `glass-lint-core/src/project/types/input/resolution.rs:53-55, 82-84`

`SourceFile::from_relative` (input.rs:214-216), `SourceFile::into_path`
(input.rs:248-250), `SourceFile::into_source` (input.rs:252-254), and
`ResolutionRequest::range_owned` (resolution.rs:82-84, a pure forward to
`key.range_owned()`) have zero callers anywhere in the workspace (verified via
`rg` over core, project, js, obsidian, harness, cli, output). All are part of
the public API surface re-exported from `project/mod.rs`. Additionally,
`ResolutionRequestKey::range_owned` (resolution.rs:53-55) is called only from
tests (`project/session/artifacts/tests.rs:121`, `project/tests/mod.rs:93`)
that rebuild an "unknown" key. This public surface must be maintained and
reviewed even though it is dead, and callers can already express the owned
range via `range().clone()` since `SourceRange: Clone`.

**Recommendation:** Delete `SourceFile::from_relative`, `SourceFile::into_path`,
`SourceFile::into_source`, and `ResolutionRequest::range_owned`; move
`ResolutionRequestKey::range_owned` under `#[cfg(test)]` or delete it and let
tests clone via `range().clone()` (callers at `artifacts/tests.rs:121` and
`project/tests/mod.rs:93`). Guardrail: keep `with_language` and
`from_relative_with_language` (production callers at
`glass-lint-project/src/boundary.rs:241`, `glass-lint-cli/src/lint.rs:107`,
`glass-lint-harness/src/adapters.rs:81, 116`) and the `path()`/`language()`/
`source()` accessors; retain `SourceFile::new` only as the documented JS-default
convenience for tests and doctests (see READ-003).

**Fix Applied:** Deleted `SourceFile::from_relative`, `SourceFile::into_path`, `SourceFile::into_source`, and `ResolutionRequest::range_owned`; deleted `ResolutionRequestKey::range_owned` and switched the two test callers to `range().clone()`.

#### [x] READ-002 — `ResolverOutcome` and `LinkedModuleTarget` are parallel enums joined by a mechanical passthrough

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/types/input/resolution.rs:91-99, 122-130`; `glass-lint-core/src/project/analysis/.../model.rs:206-224`

`ResolverOutcome` (resolution.rs:91-99) and the internal `LinkedModuleTarget`
(resolution.rs:122-130) share five of six variants verbatim
(`External`, `Builtin`, `Missing`, `OutsideProject`, `Unsupported`); only
`Internal` differs (`ProjectRelativePath` → `ModuleId`). `resolve_record`
(model.rs:206-224) is a one-to-one `match` that passes five variants through
unchanged and remaps the sixth. Every new target kind must be added to both
enums and the mapping, and the two shapes invite drift in provider/phase code
that matches on either.

**Recommendation:** Keep the phase boundary (authored path before linking vs.
assigned `ModuleId` after) — do not collapse the two lifecycles. Extract the
five shared payload variants (`External`, `Builtin`, `Missing`,
`OutsideProject`, `Unsupported`) into a single `ResolvedTargetKind` enum beside
`ResolverOutcome` in `resolution.rs`, embed it in both `ResolverOutcome`
(`Internal { path }` + passthrough target) and `LinkedModuleTarget`
(`Internal { id }` + passthrough target), and reduce `resolve_record`
(model.rs:206-224) to the `Internal` remap plus a `From` conversion, so a new
target kind is declared in one place. Update every consumer in the same change
(`glass-lint-project/src/resolver.rs`, `loader_phases.rs`, and core
`analysis/project/linker/*`). Guardrail: `ResolverOutcome` is the public
authored-input contract and must stay path-based; `LinkedModuleTarget` must stay
id-based and internal (`pub(crate)`, `project/mod.rs:26`).

**Fix Applied:** Extracted the five shared payload variants into `ResolvedTargetKind` beside `ResolverOutcome` in `resolution.rs`; both `ResolverOutcome` (`Internal { path }` + `Target`) and `LinkedModuleTarget` (`Internal { id }` + `Target`) embed it, with `From` conversions; reduced `resolve_record` to the `Internal` remap and updated every consumer in core, `glass-lint-project` (`resolver.rs`, `resolver/tests.rs`), and `glass-lint-harness` (`types/protocol.rs`).

#### [x] READ-003 — `SourceFile` exposes two JS-default constructor paths with no vocabulary difference

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/project/types/input.rs:195-226`

`SourceFile::new` (raw `impl Into<String>` path + fallible) and
`SourceFile::from_relative` (validated `ProjectRelativePath` + infallible) both
default the language to JavaScript and differ only in where the path validation
happens, yet `from_relative` has no callers (see READ-001). Production callers
always go through `with_language` or `from_relative_with_language`
(`glass-lint-project/src/boundary.rs:241`, `glass-lint-harness/src/adapters.rs:81`),
so the JS-default raw-path constructor is a convenience kept alive by tests and
doctests.

**Recommendation:** Fold this into READ-001: delete `from_relative`, and
decide explicitly whether `new` stays as the documented JS-default convenience
(kept for doctests and tests) or is removed in favor of `with_language`.
Guardrail: keep `from_relative_with_language`, which is the only validated-path
constructor with a production caller.

**Fix Applied:** Folded into READ-001: `from_relative` was deleted there, and per the resolved Open Question `SourceFile::new` stays as the documented JS-default convenience for doctests and tests; `from_relative_with_language` remains the validated-path constructor.

### Project session (`project/session/mod.rs`, `project/tables.rs`)

#### [ ] READ-004 — `SourceTable::insert` duplicates `admit_all` with fabricated limits and is exposed publicly

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/tables.rs:22-35, 39-85`; callers `glass-lint-core/src/project/session/artifacts/tests.rs:43, 59, 92`

`SourceTable::insert` (tables.rs:22-35) repeats the duplicate-detection and
checked byte-accounting of `admit_all` (tables.rs:39-85) but hardcodes
`limit: usize::MAX` and fabricates `attempted: usize::MAX` in its overflow
branch, so it never enforces a real limit and reports nonsense values if it ever
fires. Its only production role is `admit_all`'s staging loop (`staged.insert`
at tables.rs:51), where the duplicate check can never fire (the staged table
starts empty) and the overflow values are unreachable; its remaining direct
callers are tests (`artifacts/tests.rs:43, 59, 92`). The byte-accounting
invariant (admitted bytes tracked alongside the map) is duplicated between the
public `insert` and `admit_all`'s own limit checks (tables.rs:64-82), so the
`insert` half is never exercised against real limits.

**Recommendation:** Make `insert` private — a staging helper used only by
`admit_all` (or inline its two accounting lines into `admit_all`'s loop) — and
rewrite the three tests to admit through `admit_all([source], usize::MAX,
usize::MAX)`, leaving one public admission path that enforces real limits.
Guardrail: preserve atomic admission and the duplicate-rejection behavior of
`admit_all`; keep `source_bytes` accounting exactly as is so deterministic
admission limits do not change.

**Fix Applied:** None so far.

#### [ ] READ-005 — `analyze_pending_sources` re-implements `ResolutionRequestKey`'s derived ordering field by field

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Conversion
- **Location:** `glass-lint-core/src/project/session/mod.rs:362-375`; `glass-lint-core/src/project/types/input/resolution.rs:16-20`

The final deterministic sort builds the tuple
`(importer().as_str(), kind(), range(), specifier().as_str())`
(`session/mod.rs:362-375`), which duplicates the field order of
`ResolutionRequestKey`'s derived `Ord` (resolution.rs:16-20) and then appends
the specifier. `ResolutionRequestKey` is already `Ord`, so the importer/kind/
range prefix is a re-encoding of `key()`. If the key's field order or set of
fields ever changes, this sort silently diverges from the key's canonical
ordering while staying deterministic across worker counts.

**Recommendation:** Sort by `left.key()`/`right.key()` (the `&ResolutionRequestKey`
reference is `Ord`) and the specifier, e.g. compare
`(left.key(), left.specifier())` vs `(right.key(), right.specifier())`, removing
the duplicated destructure. Guardrail: keep the sort total and deterministic so
reported request order stays worker-count independent.

**Fix Applied:** None so far.

#### [ ] READ-006 — `ProjectInputError::InvalidPath` is overloaded for a missing-source lookup

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/session/mod.rs:249-272, 258`; `glass-lint-core/src/project/types/input/errors.rs:23-29`

`analyze_source_at_path_with_observer` reports a path that is absent from the
source table as `ProjectInputError::InvalidPath(path)` (session/mod.rs:258).
That variant elsewhere means "malformed project path" (`project/input.rs:37, 44,
55, 84`), so one error now covers two unrelated conditions: unparsable input and
"source not admitted". From the public `analyze_source`
(`session/mod.rs:233-237`) the lookup error is unreachable because the source is
admitted immediately beforehand, so the surface is also misleading about what
can actually fail.

**Recommendation:** Report the missing lookup as the phase error
`ProjectPhaseError::UnknownImporter(path)` — whose `Display` already reads
"resolution importer is not a source" — instead of `InvalidPath`, changing the
internal `analyze_source_at_path*` return types and their callers
(`session/mod.rs:233, 236, 246, 278, 287`; `tests/cache_and_session.rs:190`) in
the same change. Guardrail: the public `analyze_source`/`analyze_sources`
signatures and the `ProjectError` boundary must not change; `InvalidPath` must
keep rejecting malformed paths.

**Fix Applied:** None so far.

#### [ ] READ-007 — `SessionState` carries `cfg(test)` fingerprint knobs and a duplicated fingerprint construction

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/project/session/mod.rs:36-47, 73-108`

`SessionState` embeds `#[cfg(test)]` fields `fingerprint_engine_version` and
`fingerprint_normalization` (session/mod.rs:43-46) plus a separate
`artifact_fingerprint` body (session/mod.rs:73-108) that re-implements the
production `ArtifactCacheKey::new` construction (session/mod.rs:105-108) with
two override branches and a `map_or_else` chain. The setters live on
`ProjectSession` (`set_fingerprint_engine_version`/`set_fingerprint_normalization`,
session/mod.rs:417-425) and are used by `tests/cache_and_session.rs:316, 322`.
Any change to production cache-key construction must be mirrored in the test
body, and the struct layout changes between test and non-test builds.

**Recommendation:** Collapse the two `artifact_fingerprint` bodies
(session/mod.rs:73-108) into one function used by all builds: the production
`ArtifactCacheKey::new(source, environment, limits)` call becomes the fall-through,
and the two `#[cfg(test)]` override branches (`ArtifactCacheKey::for_engine_version`,
`for_test_inputs`) become early returns compiled only under `cfg(test)`. Production
cache-key construction then exists exactly once instead of being mirrored by the
test fast path. Keep the existing `cfg(test)` fields and setters
(session/mod.rs:417-425) as the injection point. Guardrail: do not change
`ArtifactCacheKey` semantics, the parse-once invariant, or the
cache-hit/miss behavior observed by tests (`tests/cache_and_session.rs:316, 322`).

**Fix Applied:** None so far.

### Project report combination (`project/report/mod.rs`)

#### [ ] READ-008 — `AnalysisReport::combine` duplicates the duplicate-path detection loop

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/project/report/mod.rs:66-72, 86-92`

`combine` runs the identical `paths.insert(file.path().clone())` / 
`DuplicateFilePath` guard twice: once over the first report
(report/mod.rs:66-72) and again over each remaining report (report/mod.rs:86-92).
The schema/version checks and the per-report path scan have to be kept in sync
manually, and the first-report handling differs only because it seeds the
accumulator.

**Recommendation:** Extract one helper that checks and records a report's file
paths into the `BTreeSet` and returns `Result<(), ReportCombineError>`, then
call it for the seed and every later report. Guardrail: preserve the exact
`DuplicateFilePath` error, `Empty` on zero reports, and the schema-before-tool
check order so combine stays lossless and deterministic.

**Fix Applied:** None so far.

### Project session tests

#### [ ] READ-009 — Repeated analyzer setup and "unknown key" rebuilds in tests

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Testing
- **Location:** `glass-lint-core/src/project/session/artifacts/tests.rs:8-14, 77-80, 98-100, 113-116, 117-122`; `glass-lint-core/src/project/tests/mod.rs:89-94`

`artifacts/tests.rs` builds a `SemanticAnalyzer` with default environment and
limits and calls `analyze_source` inline three times (lines 77-80, 98-100,
113-116) even though the module already defines a `lower` helper (lines 8-14)
that does exactly that and returns the analyzed artifact. The "make a request
unknown" rebuild — clone `importer()`, switch kind to `Require`, reuse
`range_owned()` — is duplicated verbatim between `artifacts/tests.rs:117-122`
and `project/tests/mod.rs:89-94`.

**Recommendation:** Rewrite the inlined analyzer calls to use `lower`, and lift
the unknown-key rebuild into a small shared test helper used by both modules.
Guardrail: keep the assertions on `UnknownRequest`/`DuplicateResolution`
semantics and the deterministic key construction unchanged.

**Fix Applied:** None so far.

### Documentation

#### [ ] READ-010 — Garbled module documentation in `project/input.rs`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Documentation
- **Location:** `glass-lint-core/src/project/input.rs:1-6`

The module doc comment (input.rs:1-6) contains a truncated duplicate: line 3
("The staged project session owns the canonical normalization pipeline via") is
a dangling fragment of the rewritten line 4 ("The private project-session module
owns the canonical normalization pipeline."). The file is the canonical
normalization utility surface, so the contradiction about who owns the pipeline
is actively confusing for the next reader.

**Recommendation:** Replace lines 1-6 with a single accurate statement describing
the two normalization functions and their callers (`ProjectRelativePath::new`,
`NormalizedOutsidePath::new`). Guardrail: no behavior change; keep the functions'
current visibility and error vocabulary.

**Fix Applied:** None so far.

## Systemic Themes

- **Public API surface runs ahead of real callers.** Several exported input
  types carry accessors and constructors that only tests (or nobody) call —
  `SourceFile::from_relative`/`into_path`/`into_source`,
  `ResolutionRequest::range_owned`, `ResolutionRequestKey::range_owned`.
  `SourceTable::insert` is public but serves only `admit_all`'s internal
  staging (tables.rs:51) and test callers, with fabricated limit values that
  can never fire. The `project/mod.rs:17-25` re-export list is large and
  should be reconciled against `glass-lint-project` and harness callers.
- **Parallel vocabularies across the resolution/linking boundary.** The
  authored-outcome enum (`ResolverOutcome`) and the linked-target enum
  (`LinkedModuleTarget`) describe the same domain with one payload change, and
  the `ResolutionTable`/`ResolutionCache` key/value shapes mirror across the
  crate boundary (`tables.rs:120`, `glass-lint-project/src/loader_phases.rs:54`).
  This is a deliberate phase split but it must be explicitly maintained.
- **Staged error boundary is well-shaped but one variant leaks its phase.**
  `ProjectInputError::InvalidPath` spans both "malformed path" and "unknown
  source" (session/mod.rs:258), the only place an input error doubles as a phase
  condition.
- **Test seams duplicate production logic.** The `cfg(test)` fingerprint branch
  in `SessionState` and the inlined analyzer setup in `artifacts/tests.rs` both
  re-encode production behavior that a single canonical helper would express
  once.

## Open Questions

- Resolved: `SourceFile::new` is the JS-default entry point used by unit tests,
  integration tests, and doctests only (`lint/linter.rs:228`,
  `project/report/mod.rs:53-54`); `rg` finds no production caller in any crate,
  and production always passes an explicit language via `with_language` or
  `from_relative_with_language`. It is therefore a deliberate test/doctest
  convenience, not leftover API: keep it as documented surface (per READ-003)
  and let READ-001 delete only the truly dead `from_relative`/`into_path`/
  `into_source`/`range_owned`.
- Resolved: consolidating `LinkedModuleTarget` with `ResolverOutcome` over a
  shared `ResolvedTargetKind` (README-002) is feasible in one coordinated
  change: `ResolverOutcome` is consumed in `glass-lint-project`
  (`resolver.rs`, `loader_phases.rs`) and `LinkedModuleTarget` is consumed only
  within this crate (`analysis/project/model.rs`, `linker/*`, `identities.rs`).
  The shared enum keeps the phase split intact — `ResolverOutcome` stays
  path-based and `LinkedModuleTarget` stays id-based — so no contract decision
  is deferred to the linking owners; the only requirement is updating every
  consumer in the same change.

## Coverage

Audited chunk files: `project/mod.rs`, `project/input.rs`,
`project/report/mod.rs` (chunk 24's `combine`/`ReportCombineError`),
`project/session/mod.rs`, `project/session/artifacts.rs`,
`project/session/execution.rs`, `project/tables.rs`, `project/types/mod.rs`,
`project/types/input.rs`, `project/types/input/errors.rs`,
`project/types/input/resolution.rs`, plus `project/types/input/tests.rs`,
`project/session/artifacts/tests.rs`, and `project/tests/*` where they exercise
chunk-owned APIs. Traced external callers in `glass-lint-project`
(`loader.rs`, `loader_phases.rs`, `resolver.rs`, `boundary.rs`), `glass-lint-harness`
(`adapters.rs`), `glass-lint-cli` (`lint.rs`), and in-crate consumers
(`lint/linter.rs`, `analysis/model/module.rs`, `analysis/project/model.rs`,
`lint/report/*`). `project/types/report/*` (chunk 25) was treated as an
external contract and not audited except for `AnalysisReport::combine`'s use of
`schema_version`/`tool_version`/`files`/`merge`/`finalize`.

Representative call sites verified: `SourceFile::with_language`
(`glass-lint-cli/src/lint.rs:107`, `glass-lint-harness/src/adapters.rs:81, 116`);
`SourceFile::from_relative_with_language` (`glass-lint-project/src/boundary.rs:241`);
`ProjectSession::analyze_sources`/
`finish` (`glass-lint-project/src/loader.rs:355, 415`); `AuthoredRequests::len`
(`glass-lint-project/src/loader.rs:289`); `ResolverOutcome` construction and
consumption (`glass-lint-project/src/resolver.rs`, `analysis/project/model.rs:206`);
`ResolutionRequestKey::new` production path
(`analysis/model/module.rs:418-421`); `SourceTable::insert` callers
(`tables.rs:51` staging inside `admit_all`; test callers
`project/session/artifacts/tests.rs:43, 59, 92`).
