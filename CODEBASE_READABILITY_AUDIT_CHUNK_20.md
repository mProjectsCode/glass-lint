# Codebase Readability Audit — Chunk 20

## Summary

Chunk 20 covers project sessions and phase transitions, local execution,
source and resolution tables, project input types, report combination, and
the public analysis-report model. The project boundary is otherwise clear:
sources are owned and normalized, resolution outcomes are validated against
authored requests, phase types are consumed in order, and report contents are
sorted before combined output is returned.

The concrete issues found here are concentrated in execution ownership and
stale or duplicated input identity. Project local analysis collects all
uncached jobs before applying its worker window, and its worker-panic error
contract is not implemented around the lowering closure. Internal linked
targets retain a path that all consumers ignore after resolving the module ID,
and `UnknownImporter` remains a public error variant with no construction
site after request-key validation became the active boundary.

Earlier findings READ-083 and READ-084 cover the `finish_local` transition and
the `AuthoredRequestTable::qualified_ids` `filter_map` respectively; they are
not repeated here. READ-079 covers release-mode construction of empty evidence
traces and is likewise not repeated.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Local execution admission

#### [x] READ-097 — Preserve the bounded job window through local execution

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Resource bounding / Execution lifecycle / Ownership
- **Location:** `glass-lint-core/src/project/session/mod.rs:287-310`,
  `glass-lint-core/src/project/session/execution.rs:171-216`

`ProjectCollection::analyze_pending_sources_with` first collects every
pending path and every uncached `LocalJob` into vectors. `ThreadLocalJobExecutor`
then collects the job iterator into `all_jobs` before taking batches of
`outstanding_job_bound`. The Rayon work window limits active batches, but the
full pending job set—including cloned source handles, paths, and cache keys—
is already resident before the first batch runs. This bypasses the bounded
admission intent in the session execution API and makes memory grow with the
entire project rather than with the worker window.

**Recommendation:** Make the executor own the refill loop over the job
iterator, or pass a bounded producer that admits at most one window at a time
and releases each window before reading the next. Keep deterministic path
ordering, cache-hit handling, observer accounting, and result merging
unchanged. Delete the `uncached` and `all_jobs` whole-project materialization
once one execution owner controls admission and batching.

**Fix Applied:** Local execution now streams source candidates into the
executor. A session-owned callback filters completed artifacts, handles cache
hits immediately, and admits only lowering jobs into the bounded worker
window; the executor no longer collects the complete uncached project before
starting work.

**Verification:** `cargo test -p glass-lint-core project::tests --lib`
(48 passed) and `make fmt && make ci` (passed).

### Local execution errors

#### [ ] READ-098 — Make the project worker-panic error contract real

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Error handling / Panic boundary / API
- **Location:** `glass-lint-core/src/project/session/execution.rs:200-216`,
  `glass-lint-core/src/project/types/input.rs:473-490`

`LocalExecutionError::WorkerPanic` documents a panic-safe result path, and
`ProjectCollection::analyze_pending_sources_with` maps executor errors into
`ProjectInputError::LocalExecution`. However, the Rayon closure calls
`lowerer.lower_source` directly inside `pool.install`; no
`catch_unwind`/panic-to-result boundary exists there. A panic from lowering
therefore unwinds the project-session caller instead of producing the declared
error, unlike `Linter::lint_batch`, which explicitly catches panics around
`lint_source`. The error enum also mentions channel failure even though this
executor has no channel path.

**Recommendation:** Put the panic boundary in the owning executor and turn a
panicking job or pool operation into `LocalExecutionError::WorkerPanic`, while
still releasing a result for every admitted job or explicitly synthesizing
missing results. Align the error documentation with the actual executor
mechanism and remove the stale channel wording. Preserve ordinary parse
diagnostics as per-job results and keep the caller’s phase transition
fallible rather than unwinding.

**Fix Applied:** None so far.

### Linked target identity

#### [ ] READ-099 — Remove the unused path from internal linked targets

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Domain model / Identity ownership / API
- **Location:** `glass-lint-core/src/project/types/input.rs:452-471`,
  `glass-lint-core/src/analysis/project/model.rs:181-198`

`LinkedModuleTarget::Internal` stores both `ModuleId` and the original
`ProjectRelativePath`. `resolve_record` needs the path only to look up the
stable module ID, but after constructing the target every linker, resolver,
graph, and identity caller matches `Internal { id, .. }`; none reads the
stored path. The linked project already stores each module’s path in its
`ProjectModule`, so the target duplicates an identity value owned elsewhere
and invites disagreement if a future transformation changes one copy.

**Recommendation:** Resolve the path to `ModuleId` at the conversion boundary
and make the internal target carry only the qualified ID. Obtain a diagnostic
or display path from the owning project module when needed. Delete the
duplicate field and update all pattern matches together, preserving the
outside-project path field because that path is itself the semantic target for
that variant.

**Fix Applied:** None so far.

### Project input errors

#### [ ] READ-100 — Remove the unreachable `UnknownImporter` error variant

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Dead API / Error taxonomy / Ownership
- **Location:** `glass-lint-core/src/project/types/input.rs:492-521`

`ProjectInputError::UnknownImporter` has no construction site in the workspace;
the active resolution boundary validates the complete
`ResolutionRequestKey` against `AuthoredRequestTable` and returns
`UnknownRequest`, while source admission rejects duplicate or invalid paths.
Keeping both variants leaves callers with two names for a distinction the
current session no longer produces and makes the public error taxonomy imply
an unimplemented importer-validation path.

**Recommendation:** Remove `UnknownImporter` and its display branch after
confirming downstream consumers do not pattern-match it, or reintroduce it
only at a specific importer-validation boundary that can actually construct
it. Preserve `UnknownRequest` for unauthored request identities and update
public documentation/tests in the same migration; do not add a compatibility
variant merely to retain the obsolete name.

**Fix Applied:** None so far.

## Systemic Themes

The final project boundary has strong typed transitions, but execution
admission and error conversion are split between session orchestration and
the worker runtime. That split lets an apparent bound cover only active work
while all pending work is already materialized, and lets a declared panic
error exist without an unwind boundary. The input model also retains values
after their identity has been reduced to a project-owned key, and one stale
error variant survives after the request-key contract replaced it.

READ-097 is marked applied above.

## Open Questions

- The project session owns the source table already, so a bounded executor may
  pass borrowed source data only if Rayon’s lifetime and release protocol can
  remain safe; otherwise retain cheap `SourceFile` handles while bounding job
  metadata and admitted work.
- Panic handling should decide whether one panicking job cancels the whole
  project or synthesizes a failure for only that source; either choice must
  preserve the consuming phase contract and deterministic diagnostics.
- If `LinkedModuleTarget::Internal`’s path was intended for debugging or
  diagnostics, expose that through the owning module lookup rather than
  retaining a second mutable identity.
- The 20-chunk audit is complete; all numbered artifacts are now present.

## Coverage

Reviewed the Chunk 20 modules listed in `CODEBASE_STRUCTURE_CORE.md` across
project session phases, artifact and source/resolution tables, local execution,
project input and resolver-outcome types, report combination, analysis-report
completion/summary, diagnostics, evidence, findings, locations, and operation
counts. Representative callers were traced through linter project creation,
local lowering, authored-request validation, linker target conversion, report
assembly, batch execution, and report-combination tests. Prior findings
READ-079, READ-083, and READ-084 were checked to avoid repeating their root
causes. No source, test, configuration, dependency, or documentation changes
were made.
