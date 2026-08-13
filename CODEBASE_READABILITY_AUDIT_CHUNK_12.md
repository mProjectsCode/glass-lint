# Codebase Readability Audit

## Summary

Chunk 12 owns the filesystem-free project contract: validated source and
resolution inputs, staged local-analysis transitions, bounded local execution,
artifact/request tables, and public report values. The phase-state boundaries
are a good fit for preserving authored identities and incomplete-analysis
status, but report combination repeats a full traversal, the project loader
recreates an executor pool for every wave, and two public input boundaries
expose or repeat lower-level normalization/construction mechanics.

## Findings

### Report combination

#### [ ] READ-083 — Combine owned reports in one validating accumulator pass

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Performance / Architecture
- **Location:** `glass-lint-core/src/project/report/mod.rs:43-94`, callers in `glass-lint-cli/src/output.rs:204-205,423-425`

`AnalysisReport::combine` first collects every owned report into a `Vec`,
validates schema, tool version, and duplicate file paths in a complete pass,
then creates a second iterator over the same reports to merge them. The
reports are already owned by the operation, so retaining the whole vector and
revisiting it does not provide transactional safety: an error simply drops the
partially accumulated owned value. The API therefore pays an additional
collection and traversal on the CLI path that combines every project/file
report before output.

**Recommendation:** Let an internal report accumulator establish the first
report’s schema/tool contract, validate each subsequent report and its paths,
and merge it immediately; return the same typed error while dropping the
owned accumulator on failure. Delete the intermediate `Vec` and second merge
pass, while preserving empty-input behavior, first-report error expectations,
duplicate-path rejection, saturating operation-count addition, completion
joining, final file/diagnostic ordering, and refreshed aggregate metrics.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed. Owned reports can be validated
and merged transactionally in one consuming pass because failure drops the
partial accumulator; preserve all current error ordering and finalization.

### Project local-execution lifecycle

#### [ ] READ-084 — Reuse the local-analysis executor across project waves

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Performance / Architecture
- **Location:** `glass-lint-core/src/project/session/execution.rs:188-264`, `glass-lint-core/src/project/session/mod.rs:320-378`, caller in `glass-lint-project/src/loader.rs:313-377,440-447`

`ThreadLocalJobExecutor::execute` constructs and drops a new Rayon
`ThreadPool` on every invocation. `ProjectSession::analyze_sources` invokes
that executor for one bounded source set, while `ProjectLoader::close_frontier`
calls `process_wave` repeatedly and `analyze_wave` invokes
`analyze_sources` once per wave. A multi-wave project consequently repeats
worker-pool construction and teardown even though the session’s analyzer,
cache, worker limit, and bounded execution contract remain live across all
waves; the current tests verify output and concurrency bounds but do not cover
this lifecycle cost.

**Recommendation:** Give the project-session execution owner a reusable
executor/pool for the session, or pass a loader-owned execution context through
the internal wave path, rebuilding only when the requested worker limit
actually changes. Delete per-wave pool creation while preserving the
`outstanding_job_bound`, available-parallelism cap, panic-to-typed-error
conversion, cache callback ordering, deterministic request sorting, and the
ability for public callers to request different worker limits without sharing
state across independent sessions.

**Fix Applied:** None so far.

**Audit disposition (2026-08-13):** Confirmed with the ownership decision
below: reuse belongs in the filesystem loader’s private wave context, not in
the public session API whose calls may request different worker limits.

### Validated project-input boundary

#### [x] READ-085 — Keep path normalization behind validated path types

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API / Encapsulation
- **Location:** `glass-lint-core/src/project/mod.rs:8-17`, `glass-lint-core/src/project/input.rs:26-89`, `glass-lint-core/src/project/types/mod.rs:59-63`, `glass-lint-core/src/project/types/input.rs:180-189`

The public `project::input` module exposes `normalize_relative`, which already
returns the canonical `ProjectRelativePath`, and `normalize_outside_target`,
which returns a raw normalized `String`. The public domain constructors
`ProjectRelativePath::new` and `NormalizedOutsidePath::new` already own those
same validations, and the only workspace callers of the helpers are those
constructors plus a test-only session utility. This leaves a raw normalization
API that lets callers bypass the named outside-path invariant and makes the
private representation pipeline look like a second public construction path.

**Recommendation:** Make the normalization helpers crate-private and let the
validated newtypes be the public construction boundary; replace the test-only
session call with `ProjectRelativePath::new`. If an external normalization
utility is intentionally required, return its semantic type rather than a
`String` and document its error/absolute-path contract. Preserve backslash
normalization, relative `..` rejection, outside-target parent resolution,
absolute/UNC and drive-prefix handling, NUL/empty rejection, and the exact
original value retained in validation errors.

**Fix Applied:** Made raw normalization helpers crate-private and routed the
test-only session utility through `ProjectRelativePath::new`; validated path
types remain the public construction boundary and normalization behavior is
unchanged. Verified with `make fmt && make ci`.

**Audit disposition (2026-08-13):** Confirmed. Keep validated path newtypes as
the public construction boundary and make raw normalization helpers internal;
do not alter normalization or error-original preservation.

### Source-file constructors

#### [x] READ-086 — Centralize `SourceFile` construction after path validation

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication / API
- **Location:** `glass-lint-core/src/project/types/input.rs:234-295`

The four public `SourceFile` constructors duplicate the same assembly logic:
`new` and `with_language` each convert and validate a path before building the
struct, while `from_relative` and `from_relative_with_language` repeat the
validated-path/source/language field construction. The overloads are useful
ergonomic boundaries, but the duplicated field assembly creates four places
where future source metadata or ownership changes must remain consistent.

**Recommendation:** Add one private `from_parts` constructor owning the
validated path, language, and `SourceText`, then delegate all four public
constructors to it; let the default-language variants delegate to their
explicit-language counterparts where the input contract permits. Delete the
repeated struct literals without merging the validated and unvalidated path
APIs. Preserve JavaScript as the virtual-source default, explicit TypeScript
selection, extension-independent language semantics, source allocation reuse,
and the distinction between fallible raw-path and infallible validated-path
constructors.

**Fix Applied:** Added private `SourceFile::from_parts`; raw-path
constructors still validate first, validated-path constructors delegate, and
defaults/language/source ownership are unchanged. Verified with `make fmt &&
make ci`.

**Audit disposition (2026-08-13):** Confirmed. A private parts constructor
removes repeated field assembly without merging the validated and raw-path
contracts.

## Systemic Themes

- The consuming session phases are valuable ownership boundaries: local
  analysis must finish before authored resolutions are validated, and linking
  must receive typed identities rather than raw paths or resolver strings.
  Findings READ-085 and READ-086 narrow mechanics at those boundaries without
  collapsing the phase types.
- Bounded execution is correctly enforced in batches and waves, but executor
  lifetime is currently shorter than the project session. Reusing the worker
  resource should not change the explicit work bound or deterministic release
  behavior.
- Report values preserve deterministic ordering and aggregate metrics, but
  report combination can own validation and merging in one stateful pass
  because its input reports are already consumed.
- The project input and report types generally hide storage and retain typed
  error domains. The remaining API opportunities are leaked normalization
  helpers and repetitive ergonomic constructors, not a need to expose maps,
  artifacts, or phase internals.

## Open Questions

- None remain. READ-084 belongs in the filesystem loader’s private wave
  context: its repeated waves use one normalized worker limit, while the
  public `ProjectSession` API must remain free to honor different limits on
  independent calls without sharing executor state.
- No prior finding was duplicated: READ-059 covered duplicate matcher project
  identity wrappers, while READ-085 covers public path-normalization ownership
  and READ-086 covers `SourceFile` constructor assembly.

## Coverage

- Reviewed: `project::input`, `project::tables`, `project::session`,
  `project::session::artifacts`, `project::session::execution`,
  `project::types::input`, `project::types::report`, and `project::report`;
  the core session bridge, filesystem-loader wave caller, CLI report callers,
  public exports, architecture boundaries, and project/report tests.
- Verification: `cargo test -p glass-lint-core project::` (112 passed),
  `cargo test -p glass-lint-core --test integration public_surface` (3
  passed), `cargo test -p glass-lint-core --test integration typescript` (9
  passed), and `cargo test -p glass-lint-project` (65 passed).
- No source, test, configuration, or dependency file was modified. This chunk
  artifact was updated with review dispositions only.
- Historical audit chain: Chunk 11 ended at READ-082. This final Chunk 12
  artifact continues with READ-083 through READ-086; all 12 structure chunks
  now have corresponding audit files.
