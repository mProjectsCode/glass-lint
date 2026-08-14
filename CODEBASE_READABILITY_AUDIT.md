# Codebase Readability Audit

## Summary

`glass-lint-core/src/lint/report` has a coherent pipeline, but its ownership is
spread across three phase structs and several free functions. The same project
state is destructured and reconstructed at each phase, while evidence and
diagnostic helpers receive that state piecemeal. A staged report assembler with
small owned sub-assemblers would remove most plumbing while preserving the
linking/matching lifecycle and the final report contract.

## Findings

### Report pipeline state

#### [x] READ-001 — Consolidate duplicated phase state behind one report assembler

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/lint/report/mod.rs:63-289`; `glass-lint-core/src/project/session/mod.rs:428-451`

`LinkedReport`, `MatchedReport`, and `RenderedReport` each store most of the
same `ProjectSemanticModel`, `ProjectReportSession`, file map, and timing state.
The transitions then destructure one stage and rebuild the next (`mod.rs:187-224`,
`230-251`, and `257-288`). This makes the phase lifecycle look like data
plumbing, exposes three intermediate types for a single internal caller, and
leaves status/trace mutation split between `ProjectReportSession` methods and
the outer transition (`mod.rs:199-213`).

**Recommendation:** Make one private `ProjectReportAssembler` (or equivalent)
own the project, session, file collection, timings, and phase-specific
artifacts, with methods such as `link`, `match_rules`, `render_findings`, and
`finish`; remove the three duplicated stage structs if no independent caller
needs them. Let the assembler own status transitions and trace-arena
installation rather than assigning `session.status` and `session.trace_arena`
from the coordinator. Keep linking and matching as explicit lifecycle phases,
retain separate timings, and preserve the current fail-closed behavior when
classification fails. The final `ProjectAnalysis` and `AnalysisReport` APIs
should remain the only externally meaningful result boundary.

**Fix Applied:** Replaced the three duplicated phase structs with one private
`ProjectReportAssembler` that owns linking, matching, evidence rendering, and
finalization. Status failure recording and trace-arena installation now belong
to `ProjectReportSession`; `ProjectSession::finish` calls the assembler
directly. Verified with `cargo test -p glass-lint-core project::report` and the
requested `make fmt && make ci` gate.

### Evidence assembly

#### [x] READ-002 — Give evidence rendering an owner for shared report context

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/lint/report/evidence.rs:51-56,92-109,153-169,265-296,298-321`

Evidence construction repeatedly passes the same report context through five
layers: `catalog`, `project`, `session`, module/path data, and source-line
lookup. `populate_project_files` takes five parameters, while
`findings_for_capability` takes six and `FindingGroup::into_evidence` and
`resolve_trace` separately receive the project/session pair. The functions are
therefore coupled by an implicit renderer context that is not represented by a
type; adding another report-wide input will expand every layer again.

**Recommendation:** Introduce a reusable private `FindingRenderer` that owns
the catalog, project, and session references, and a short-lived module view
that owns the module path and line index while rendering that module. Move
trace resolution and fallback construction onto the renderer/context, so the
range/group types only handle range selection and occurrence grouping. Keep
classification results as inputs rather than moving matching policy into the
renderer; preserve source-order range retention, duplicate merging, evidence
truncation, and `Definite`/`Possible` certainty joins.

**Fix Applied:** Added `FindingRenderer` to own catalog, project, and trace
session context. Capability rendering, module rendering, trace reconstruction,
and fallback evidence now use renderer methods while range grouping remains
independent. Verified with focused report/linter tests and the requested
`make fmt && make ci` gate.

### File and diagnostic assembly

#### [x] READ-003 — Encapsulate the report file collection instead of passing a raw map

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/lint/report/diagnostics.rs:13-82`; `glass-lint-core/src/lint/report/evidence.rs:92-109`; `glass-lint-core/src/lint/report/summary.rs:13-49`

The file report lifecycle is distributed across three modules through a raw
`BTreeMap<ProjectRelativePath, FileReport>`. Diagnostics initialize it and later
mutate individual entries, evidence replaces entries with `FileReport::new`,
and summary finally consumes the map. The map is carrying important invariants
—one report per normalized path, parse diagnostics surviving rendering,
file-local versus project-level diagnostic routing, and deterministic path
ordering—without an owner. This also makes it easy for a future rendering
change to replace a file and accidentally discard diagnostics.

**Recommendation:** Add a private `ReportFiles` domain collection that owns the
map and exposes operations such as `from_sources`, `replace_findings`,
`push_file_diagnostic`, `push_project_diagnostic`, and `into_sorted_vec`.
Consolidate initialization, evidence insertion, and diagnostic routing around
those operations, then delete the raw-map parameters and the repeated
`FileReport::new` assembly. Preserve parse diagnostics for failed sources,
route located diagnostics to their file and unlocated diagnostics to the
project list, and keep the final normalized path order stable.

**Fix Applied:** Added private `ReportFiles` ownership for source-file
initialization, finding replacement, file/project diagnostic routing, and
deterministic final conversion. Finding replacement now preserves existing
file diagnostics, with focused unit coverage for both file-local and
project-level retention. Verified with the requested `make fmt && make ci`
gate.

## Systemic Themes

- Report phase state is modeled as repeated storage rather than as one owner
  with explicit transitions.
- Private helpers communicate through positional parameter lists instead of
  domain views that express which context they require.
- `BTreeMap<ProjectRelativePath, FileReport>` is acting as an undeclared domain
  collection, so report invariants are enforced by coordination between
  modules.

The implementation order should be READ-001, then READ-003, then READ-002:
first establish the pipeline owner, then give it an owned file collection, and
finally make evidence rendering consume those owners. Avoid introducing a
generic "context" or "builder" that merely forwards every field; each new
type should own a lifecycle or invariant and should delete call-site plumbing.

## Open Questions

- Does any downstream crate need `LinkedReport` as a deliberate extension
  point, or can it become private once `ProjectSession::finish` calls the
  assembler directly? Current repository callers only use it through the
  session path.
- Should `ProjectAnalysisTimings` remain a separate public value, or should
  timing access be folded into a future analysis result type? This is an API
  decision independent of the internal assembler refactor.
- Should evidence rendering and diagnostic routing share one report context,
  or remain separate sub-assemblers under the pipeline owner? Keep them
  separate if sharing would make either owner responsible for unrelated policy.

## Coverage

Reviewed `ARCHITECTURE.md`, `glass-lint-core/ARCHITECTURE.md`, `TESTING.md`,
`CONTRIBUTING.md`, the report pipeline and its callers, the public report types
under `glass-lint-core/src/project/types/report`, and report behavior tests in
`glass-lint-core/src/project/report/tests.rs`. No source, test, configuration,
or dependency files were changed.
