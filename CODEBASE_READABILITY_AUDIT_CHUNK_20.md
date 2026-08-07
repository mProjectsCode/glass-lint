# Codebase Readability Audit — Chunk 20

## Summary

Chunk 20 owns the filesystem-free project boundary: validated source and
resolution inputs, deterministic source/resolution tables, consuming session
phases, local-artifact staging, job execution hooks, and report values. The
phase-state design is clear at the high level: sources are admitted before
local analysis, `finish_local` freezes the authored request set, `resolve`
validates resolver answers, and `ResolvedProject::finish` is the only path to
linking and matching. The main remaining risks are weaker boundaries around
construction and errors, duplicated identity in parse diagnostics, an exposed
numeric module handle, and two public shapes for the same authored-request
result.

The source re-normalization and raw unsupported-outcome finding from Chunk 16,
the report post-finalization ordering finding from Chunk 16, and the local
cache/lowering duplication finding from Chunks 16 and 19 were checked and are
not repeated. This report focuses on the distinct project input/session/report
contracts themselves.

## Findings

### Session construction and phase errors

#### [x] READ-096 — Make project-session construction an internal, infallible boundary

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/project/session/mod.rs:38-73,207-217`; caller in `glass-lint-core/src/lint/linter.rs:95-105`

`ProjectCollection::new` is public and returns `Result<Self,
ProjectInputError>`, but its only operation is constructing empty tables and
wrapping the already-created `SessionState`; it always returns `Ok`. The
argument type is effectively crate-private because `SessionState` is only
re-exported with `pub(crate)` visibility and its constructor is
`pub(crate)`. The public method therefore cannot serve as an external
construction API, while `Linter::begin_project` is the real owner of session
creation.

This leaves an unusable public surface and an error channel that suggests
construction can reject input when no validation happens there. It also makes
the intended ownership of the environment, limits, catalog references, and
cache handle harder to see: callers must discover that the linter is the only
valid state factory.

**Recommendation:** Make `ProjectCollection::new` crate-private and return
`Self`, or move the operation onto an internal `SessionState` factory. Let
`Linter::begin_project` construct the empty phase directly and delete the
unreachable `Result`/`?` plumbing. Preserve the borrowed linter lifetime,
shared artifact cache, selected catalog and evidence limit, and all consuming
`ProjectCollection → LocallyAnalyzedProject → ResolvedProject` transitions.

**Fix Applied:** Made `ProjectCollection::new` crate-private and infallible,
and made `Linter::begin_project` the public session factory with a direct
`ProjectCollection` return. Removed the unreachable result propagation from
all callers and updated tests and adapters to use the infallible boundary.

**Verified:** `make fmt && make ci` (workspace check, clippy with warnings as
errors, 811 core tests, doctests, E2E/rule harnesses, rules documentation
check, and examples).

#### [ ] READ-097 — Separate project-input failures from session and execution failures

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/project/types/input.rs:465-529`; result boundaries in `project/session/mod.rs:219-501`, `project/session/artifacts.rs:40-164`, and `lint/batch.rs:77-97`; adapter mapping in `glass-lint-project/src/error.rs:35-48,144-148`

`ProjectInputError` is the public error for several unrelated domains. The
same enum reports malformed paths and module targets from input constructors,
duplicate sources and resolver answers from mutable tables, unknown authored
requests and incomplete phase transitions from `AnalysisArtifacts`, module-ID
budget exhaustion, and worker-pool/local-execution failure. `ProjectCollection`,
`ResolvedProject`, and `BatchResult` consequently expose the same broad error
set even though their callers are at different lifecycle boundaries; the
project crate then wraps every variant as `InvalidProjectInput`.

The name and grouping obscure recovery policy. A caller cannot tell from the
outer type whether it should reject user input, supply a missing resolver
answer, treat analysis as partial, or report an executor failure without
matching variants. New phase-specific failures will continue to accumulate in
the input enum, and adapters lose the distinction at their conversion
boundary.

**Recommendation:** Give raw input constructors a focused validation error,
give `resolve`/artifact freezing a typed resolution or phase error, and keep
local execution failure in a session/execution error. If a single public
return type is required for convenience, use an explicitly nested
`ProjectError::{Input, Phase, Execution}` wrapper with `From` conversions at
the linter boundary, rather than adding unrelated variants to
`ProjectInputError`. Preserve stable display text where needed, duplicate and
unknown-request rejection, incomplete-analysis fail-closed behavior, and the
distinction between parse diagnostics and worker failure.

**Fix Applied:** None so far.

### Identity and report contracts

#### [ ] READ-098 — Give parse diagnostics one authoritative project path

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/parse.rs:23-34`; `project/types/report/diagnostic.rs:7-49,60-123`; construction in `lint/report/diagnostics.rs:11-39`

`ParseDiagnostic` stores a public `filename: String`, while the report
`Diagnostic::Parse` variant stores a separate validated
`ProjectRelativePath`. `Diagnostic::parse` accepts both values, and
`initialize_project_files` supplies the report path from the source table
while retaining the parser diagnostic's filename. The serialized report can
therefore carry two representations of the same identity, and the public
`Diagnostic::path` accessor uses one while `parse_diagnostic().filename` uses
the other.

The normal parser path makes the strings agree, but the type contract does not
enforce that relation. A future parser or diagnostic producer can create a
file report whose grouping/path is correct while its embedded filename is
stale, non-normalized, or unrelated. Report assembly, source rendering, and
JSON consumers then have different owners for one location identity.

**Recommendation:** Keep the standalone parser DTO’s raw filename, but make
the report conversion derive and canonicalize that filename from the validated
`ProjectRelativePath` before storing or serializing it. Delete the independent
caller-supplied identity path after migration. Preserve standalone parser
diagnostics, normalized project paths, source ranges, deterministic file
grouping, and the existing parse-versus-project diagnostic distinction.

**Fix Applied:** None so far.

#### [x] READ-099 — Keep `ModuleId` opaque to project callers

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Newtype
- **Location:** `glass-lint-core/src/project/types/input.rs:440-449`; assignment in `project/tables.rs:35-50`; project/linker consumers throughout `analysis/project` and `analysis/flow/cross`

`ModuleId` is a private `u32` newtype whose constructor is crate-private,
but its public `get()` method exposes the numeric representation. IDs are
assigned by `SourceTable::module_ids` from sorted path order and are then used
as project-local keys throughout linking, identities, flow, and evidence. No
external caller in the workspace needs the raw number; the public-surface
contract exercises project behavior through paths and reports instead.

Publishing the number makes an artifact/project-local identity look stable
across sessions and invites callers to persist or compare IDs outside the
owning module. It also turns the assignment strategy in `SourceTable` into an
observable API detail, even though a future table implementation could retain
deterministic traversal without the same numeric numbering.

**Recommendation:** Remove `ModuleId::get` from the public surface or make it
crate-visible, and expose only identity-preserving operations needed by the
project owner. Keep the constructor and raw conversion inside project/linking
code, preserve `Eq`/ordering for deterministic internal maps, and retain
path-order assignment and cross-module identity comparisons within one linked
project.

**Fix Applied:** Restricted `ModuleId::get` to crate visibility. Internal
linking and trace tests retain numeric access where needed, while project
callers can no longer observe or persist the table’s numeric assignment.

**Verified:** `make fmt && make ci` (workspace check, clippy with warnings as
errors, 811 core tests, doctests, E2E/rule harnesses, rules documentation
check, and examples).

### Authored request result API

#### [ ] READ-100 — Use one domain collection for authored-resolution requests

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** API
- **Location:** `glass-lint-core/src/project/session/artifacts.rs:65-77`; single-source API in `project/session/mod.rs:234-243`; multi-source API in `project/session/mod.rs:327-344`; consumers in `glass-lint-project/src/loader.rs:409-420` and `glass-lint-harness/src/adapters.rs:113-125`

The one-source API returns `SourceAnalysis`, a public wrapper whose only
state is `Vec<ResolutionRequest>` and whose only operations are
`requests()`/`requests_ref()`. The multi-source `ProjectCollection::analyze_sources`
API returns the raw `Vec<ResolutionRequest>` instead. The project loader
consumes the raw vector, while the harness adapts the one-source wrapper and
the core linter discards it entirely.

Both results represent the same authored-request contract that must be
collected, sorted, and later matched against explicit resolver outcomes. The
two shapes force callers to learn whether to unwrap a wrapper or manipulate a
raw vector, and make it possible for future request metadata or ordering rules
to be added to only one path. The wrapper currently adds vocabulary but does
not own a distinct invariant from the vector it contains.

**Recommendation:** Replace `SourceAnalysis` and the raw multi-source vector
with one `AuthoredRequests` domain collection returned by both APIs. Put
deterministic ordering, iteration, and any future request-membership metadata
on that owner, then delete the duplicate raw `Vec`/wrapper accessors. Preserve
single-source request inspection, multi-source sorting before project
resolution, authored-key validation, and the existing consuming phase
transitions.

**Fix Applied:** None so far.

## Systemic Themes

- The phase transitions themselves are consuming and well-defined, but their
  factories and error types do not express the same ownership discipline:
  internal state construction is publicly shaped while failures from several
  phases share one input error.
- Project identity is normalized in the tables and reports, yet raw or
  duplicated identity forms remain at the parser/report and module-ID
  boundaries.
- Public project APIs should expose semantic phase values and collections,
  not the storage representation (`u32`, `Vec`, or a second filename field)
  that callers must keep coherent.

## Decisions

- `ParseDiagnostic` remains a standalone parser DTO with a raw filename so
  parser users need not construct project paths. Report assembly is the one
  conversion boundary: it derives and canonicalizes the filename from the
  validated source path, so report grouping and embedded location cannot
  disagree.
- `ProjectInputError` is too broad for the phase boundaries it serves. Split
  input validation, phase/resolution, and local execution errors, with an
  explicit outer `ProjectError` only where a convenience API needs one. The
  project adapter must preserve those distinctions instead of mapping all of
  them to invalid input.
- `SourceAnalysis` has no distinct metadata invariant today. Replace it and
  the raw multi-source vector with one `AuthoredRequests` domain collection;
  add source metadata later only by extending that owner, not by restoring two
  parallel public shapes.

## Coverage

Reviewed the full Chunk 20 scope in `CODEBASE_STRUCTURE_CORE.md`: project
report combination, session phase types and callbacks, artifact/request
tables, execution traits and worker adapters, project input and resolution
types, all report DTOs, and their core, project-crate, CLI, and harness
callers. Traced construction and consuming transitions, source/path
normalization, authored-request membership, module-ID assignment, parse and
project diagnostic assembly, report combination, and one-source versus
multi-source request handling. Inspected panic/expect/dead-code signals and
cross-checked Chunks 1–19 for duplicate root causes. No source, test,
configuration, or documentation files were changed; this audit file is the
only Chunk 20 addition.
