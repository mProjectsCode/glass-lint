# Codebase Readability Audit — Chunk 13: Cross-Cutting Core Architecture

Scope: cross-cutting review of all twelve boundaries in
`CODEBASE_STRUCTURE_CORE.md`, after resolving the decisions recorded in
`CODEBASE_READABILITY_AUDIT_CHUNK_01.md` through
`CODEBASE_READABILITY_AUDIT_CHUNK_12.md`.

This report records new architectural root findings. It does not replace the
more local recommendations in the numbered chunk reports, and no production
source was changed.

## Summary

The core modules generally have sensible local owners, but adjacent phases
often communicate by reconstructing the same concept from booleans, maps,
tuples, raw IDs, or presentation DTOs. The most important architectural work
is to make phase transitions typed and one-way while preserving the deliberate
distinctions between incomplete states, semantic evidence, public authoring
types, and internal artifact identities.

| ID | Theme | Cross-cutting root | Severity | Fix complexity |
|---|---|---|---|---|
| CROSS-001 | ENCAPSULATE | Completion state is represented independently by local, flow, linking, and report phases | High | High |
| CROSS-002 | ENCAPSULATE / DEDUPLICATE | Evidence ownership crosses matching, classification, and report layers through parallel representations | High | High |
| CROSS-003 | ENCAPSULATE | Public re-exports expose internal phase coordinators and artifact identities | High | Medium |
| CROSS-004 | DEDUPLICATE | Validation and derived-requirement ownership is repeated at neighboring phase boundaries | High | High |
| CROSS-005 | ENCAPSULATE | Core identity types lack one explicit public-versus-artifact-local ownership policy | Medium | High |
| CROSS-006 | SIMPLIFY | Final report construction has several competing owners for state, timing, and presentation | Medium | High |

## Findings

### Completion and uncertainty contract

#### [x] CROSS-001 — Give phase completion one canonical boundary contract

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture / API
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:26-145`,
  `glass-lint-core/src/analysis/lowering/mod.rs:254-315`,
  `glass-lint-core/src/analysis/flow/projector/mod.rs:70-115`,
  `glass-lint-core/src/analysis/project/projection.rs:341-365`,
  `glass-lint-core/src/analysis/lowering/status.rs:29-143`,
  `glass-lint-core/src/project/types/report/analysis_report.rs:5-30`

The core has several valid local representations of “this phase cannot claim
complete coverage”: `FactStreamIssueSet` plus `valid`, lowering capabilities
and `AnalysisStatus`, `LocalProjectionCompletion` bit flags,
`ProjectionStatus` booleans and observed counts, and the public
`ReportCompletion::{Complete, Partial}` lattice. Each is locally reasonable,
but their conversions are distributed across lowerer, linker, projection,
report-session, and report-assembly code. A new bounded failure can therefore
be recorded in one representation while a downstream derived consumer or
report path checks another.

**Recommendation:** Keep phase-local detail types where they encode different
budgets or scopes, but define one private monotonic completion adapter at each
phase boundary into the existing `AnalysisStatus`/`IncompleteReason` model.
Make `SemanticArtifact` consume a typed derived-phase capability, make
projection outcomes convert their exhaustion detail to status exactly once,
and derive `ReportCompletion` only from the finalized status rather than from
an independent boolean. Delete duplicate “is this complete?” gates once the
owning transition has consumed the adapter; do not introduce one universal
mega-enum that erases local reason, scope, or budget detail.

**Guardrails:** Incomplete analysis must remain fail-closed for definite
coverage, an independent complete witness may remain possible, status ordering
and deduplication must remain deterministic, and parse diagnostics must not
become a hidden completion side channel. Preserve lazy effects and bounded
projection behavior.

**Related local findings:** Chunk 1 READ-002/003, Chunk 3 READ-004, Chunk 5
READ-001/003, Chunk 7 READ-002, Chunk 9 READ-007, and Chunk 12 READ-002.

**Fix Applied:** Derived-phase admission is now carried by the private
`DerivedPhaseCapabilities` value retained on `SemanticArtifact`. Projection
completion uses typed monotonic states instead of parallel exhaustion
booleans, and `ProjectionOutcome` owns the single conversion of flow/effect
exhaustion into `AnalysisStatus`; report assembly only invokes that boundary.
The finalized report continues to derive `ReportCompletion` from the session
status. Verified with `make fmt && make ci`.

### Evidence lifecycle

#### [x] CROSS-002 — Establish one typed evidence pipeline across matching and reporting

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE / DEDUPLICATE
- **Category:** Architecture / Conversion
- **Location:** `glass-lint-core/src/analysis/matching/evidence.rs:20-185`,
  `glass-lint-core/src/api/classification.rs:57-83,202-325`,
  `glass-lint-core/src/lint/report/evidence.rs:20-78,165-275`,
  `glass-lint-core/src/project/types/report/evidence.rs:85-157`

Evidence moves through at least four shapes: matching accumulates and bounds
`ClassificationEvidence`, classification stores occurrences and opaque
correlation IDs, report grouping rebuilds `EvidenceRangeEntry` and
`EvidenceOccurrenceRef`, and the public report creates `EvidenceTraces` with a
separate truncation mode. The same non-empty, certainty, truncation, ordering,
and reference-validity rules are consequently enforced in multiple modules.
The report layer resolves the same occurrence references again for traces,
truncation, and certainty, while matching and the rule evidence table each
rebuild classification groups.

**Recommendation:** Make the ownership chain explicit: matching owns a
private `RawEvidenceGroup` and deterministic occurrence normalization;
classification owns validated `ClassificationEvidence` and rule-index
capacity; report assembly consumes a private resolved evidence view that owns
reference validation, range grouping, certainty, and trace construction; the
public report owns only serialized `EvidenceTraces`. Each conversion should be
one-way and named, with no lower layer reaching into report storage. Delete
positional range reconstruction and repeated occurrence resolution after the
resolved view exists.

**Guardrails:** Preserve definite-versus-possible certainty, independent
possible witnesses, total counts after truncation, fallback occurrence traces,
trace-node limits, deterministic sorting/deduplication, and the rule that
invalid internal references cannot create a finding. Keep provider-neutral
classification free of occurrence-index and project-overlay knowledge.

**Related local findings:** Chunk 6 READ-005, Chunk 9 READ-006, Chunk 11
READ-004/005, and Chunk 12 READ-004/006.

**Fix Applied:** Matching now names its private accumulated group state as
`RawEvidenceGroup`, while report assembly consumes a private
`ResolvedEvidenceOccurrence` view that holds validated evidence/occurrence
pairs directly. Range grouping, trace construction, truncation, and certainty
no longer resolve positional references through parallel scans. Verified with
`make fmt && make ci`.

### Public phase boundaries

#### [x] CROSS-003 — Audit the root public facade against internal phase ownership

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/lib.rs:19-43`,
  `glass-lint-core/src/lint/mod.rs:14-18`,
  `glass-lint-core/src/project/mod.rs:13-25`,
  `glass-lint-core/src/lint/report.rs:20-24,107-128`,
  `glass-lint-core/src/project/types/input.rs:390-429`

The root facade mixes deliberate semantic authoring APIs with types that only
make sense inside a phase transition. `VarId` is intentionally public because
rule declarations use it, but `ReportAssembly` is an internal report bridge,
`ProjectAnalysis` exposes raw timing fields, and `ModuleId`/
`LinkedModuleTarget` describe post-link identities that external callers do
not construct or access. The result is not simply a large API: callers cannot
tell which types are stable semantic contracts and which are implementation
artifacts re-exported for convenience.

**Recommendation:** Maintain an explicit facade rule: public types must be
constructible or consumable through semantic operations, while compiler IR,
linker identities, report assemblers, caches, indexes, and executor storage
remain private. Apply it first to hide `ReportAssembly` and the linker-only
`ModuleId`/`LinkedModuleTarget`, and replace raw timing fields with a documented
workspace-facing result/accessors required by `glass-lint-project`. Keep
validated rule-authoring types such as `VarId`, `RuleId`, and report DTOs public
where their semantic contracts are exercised by downstream crates.

**Guardrails:** Search all workspace callers before narrowing visibility;
preserve the public rule-authoring and serialized-report contracts, do not leak
artifact-local IDs through new accessors, and do not solve the boundary by
adding compatibility wrappers around obsolete constructors.

**Related local findings:** Chunk 8 READ-004, Chunk 9 READ-004/005, Chunk 11
READ-003, and Chunk 12 READ-003.

**Fix Applied:** The root facade now keeps `ReportAssembly` private, exposes
`ProjectAnalysis` through private report/timing fields and a named consuming
`into_parts` boundary, and keeps linker-only `ModuleId` and
`LinkedModuleTarget` crate-private. Validated authoring types and serialized
report DTOs remain public. Covered by the existing `make fmt && make ci`
verification for the boundary changes.

### Validation and derivation ownership

#### [x] CROSS-004 — Assign each phase invariant one sealing transition

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Architecture / Conversion
- **Location:** `glass-lint-core/src/api/compiler/physical.rs:145-185,278-297,540-558`,
  `glass-lint-core/src/api/classification.rs:220-325`,
  `glass-lint-core/src/project/session/artifacts.rs:103-188`,
  `glass-lint-core/src/analysis/lowering/mod.rs:349-415`

Adjacent boundaries repeatedly validate or derive the same state. Physical
roots validate themselves, `PhysicalPlan::from_roots` validates the plan, and
`validate_physical_plan` walks roots and recomputes requirements again.
`RuleEvidenceTable` repeats rule-index bounds checks in each mutation method.
`AnalysisArtifacts` infers per-path lifecycle from map membership in multiple
methods, while lowering separately re-derives completion gates for indexes and
effects. These are not identical bugs, but they share the same ownership
failure: callers cannot see which transition makes a value trustworthy.

**Recommendation:** Define a small invariant matrix for each phase and choose
one owner for each row: local constructors validate local shape, a consuming
sealer validates cross-object consistency and derives requirements, and domain
collections own repeated index/capacity admission. For example, make
`PhysicalPlan::from_roots` the production sealer, make the evidence table own a
single bounded-entry primitive, and make `AnalysisArtifacts` own one atomic
per-path outcome transition. Delete normal-path revalidation and caller-side
reconstruction, retaining test-only malformed-state constructors where they
prove the sealer's error behavior.

**Guardrails:** Keep fail-closed validation, explicit error variants, capacity
and budget limits, deterministic derived requirements, and internal malformed
state tests. Do not remove validation merely because a constructor currently
looks private; remove it only after its invariant has a named owner at the
phase boundary.

**Related local findings:** Chunk 4 READ-004, Chunk 5 READ-001/003, Chunk 6
READ-005, Chunk 8 READ-006/007, Chunk 9 READ-003/006, and Chunk 12 READ-002.

**Fix Applied:** `PhysicalPlan::from_roots` is now the production sealer that
validates roots and derives requirements once; the broader plan validator is
retained only for malformed-state tests. `RuleEvidenceTable` centralizes
rule-capacity admission in one private mutation primitive. The earlier atomic
per-path artifact outcome and derived-capability transitions complete the same
ownership pattern. Verified with `make fmt && make ci`.

### Identity ownership

#### [x] CROSS-005 — Publish an explicit policy for semantic IDs versus artifact IDs

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** API / Newtype / Architecture
- **Location:** `glass-lint-core/src/api/classification.rs:11-23`,
  `glass-lint-core/src/api/rule/query/mod.rs:50-69`,
  `glass-lint-core/src/api/rule/query/value.rs:1-35`,
  `glass-lint-core/src/project/types/input.rs:315-429`,
  `glass-lint-core/src/parse.rs:25-38`

Core uses several opaque identities with different owners: public rule
authoring uses `VarId`, compiler matching uses `ArgumentIndex` and physical
slots, classification correlates with `RuleIndex`, project linking assigns
`ModuleId`, and parser diagnostics carry an authored filename while project
reports use `ProjectRelativePath`. Some are rightly semantic public values,
some are artifact-local compiler IDs, and some are internal project identities,
but the visibility and conversion policy is not uniform. This is why a
linker-only `ModuleId` can be publicly re-exported while a compiler slot is
kept private, and why path identity can be duplicated as a string.

**Recommendation:** Document and enforce a three-way identity policy: public
semantic IDs are stable and have domain operations; phase-local IDs are
private and can only be compared within their owning artifact; boundary
identities such as `ProjectRelativePath` are validated newtypes, never raw
strings. Add explicit named conversions at phase boundaries and remove public
re-exports/accessors for artifact-local IDs. Keep separate IDs when their
allocation domains differ; the goal is not a global ID type but an obvious
owner and conversion for every identity.

**Guardrails:** Preserve artifact-local `NameId`/value identity rules, stable
rule declaration ergonomics, deterministic path/module assignment, and
standalone parser diagnostics. Never make an artifact-local ID comparable
across files or projects merely to reduce conversion code.

**Fix Applied:** The crate-level API now documents the three-way identity
policy: `VarId` and `ArgumentIndex` remain public authored semantics,
`ProjectRelativePath` remains the validated project boundary, and parser
diagnostics retain authored filenames. Classification indices, evidence
tables, cache fingerprints/keys, qualified request IDs, module IDs, and
linker targets remain behind private implementation-module boundaries. The
unused catalog rule-index reverse map and accessor were removed, so no public
catalog operation leaks an artifact-local key. Verified with `make fmt && make
ci`.

**Related local findings:** Chunk 2 READ-005, Chunk 4 READ-001/005, Chunk 7
READ-001/005, Chunk 8 READ-004/006, Chunk 9 READ-004, Chunk 10 READ-003, and
Chunk 12 READ-003/007.

### Final report ownership

#### [x] CROSS-006 — Make the finalized report pipeline the sole owner of report state

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Architecture / API
- **Location:** `glass-lint-core/src/project/session/mod.rs:448-467`,
  `glass-lint-core/src/lint/report.rs:20-186`,
  `glass-lint-core/src/lint/report/summary.rs:10-63`,
  `glass-lint-core/src/project/types/report/analysis_report.rs:35-210`,
  `glass-lint-project/src/loader.rs:533-560`

Report construction is split across `ResolvedProject`, public
`ReportAssembly`, `ProjectReportSession`, report helper modules,
`AnalysisReport`, and the project loader. The session owns status and trace
arena state, the assembler owns linking/matching/timing orchestration, summary
helpers count finalized data, `AnalysisReport::summary` recomputes aggregates,
and `glass-lint-project` attaches project diagnostics after timing is returned.
Each owner is defensible in isolation, but the final report's completeness,
diagnostic placement, operation counts, and timing contract are assembled in
different layers.

**Recommendation:** Make `ResolvedProject::finish_with_timings` the one
consuming report-pipeline boundary and use private typed transitions for link
result, match result, finalized files, and final report. Keep phase timing at
the boundary that owns the work, keep project-crate diagnostics as an explicit
outer composition step, and let one report-owned accumulator produce summary
counts and operation-count inputs where semantics overlap. Hide
`ReportAssembly`; expose only the final report and the narrow workspace timing
value.

**Guardrails:** Preserve the consuming lifecycle, parse/project diagnostic
separation, partial-report semantics, deterministic file/evidence ordering,
operation-count meaning, and `glass-lint-project`'s ability to attach its own
diagnostics. Do not make report DTOs mutable or move filesystem/project policy
into Core.

**Revalidation:** Covered by the current report pipeline. `ResolvedProject`
owns the consuming `finish_with_timings` transition, `ReportAssembly` is
`pub(super)`, and core constructs the finalized report plus phase timings in
one pipeline. The project crate's post-core `with_project_diagnostics` call is
the explicitly allowed outer composition boundary for tsconfig diagnostics.
`AnalysisReport::summary` is a read-only derived view, not a second report
state owner. No additional source change is justified without moving provider
or project policy into Core.

**Related local findings:** Chunk 5 READ-005, Chunk 7 READ-003, Chunk 10
READ-003, Chunk 11 READ-002/003/004/005, and Chunk 12 READ-005/006.

## Systemic Themes

- Core has strong local domain types, but phase adapters frequently unpack
  them into primitives before the next owner can consume them.
- The preferred architecture is a sequence of narrow, consuming transitions:
  each owner validates and enriches its state once, then hands a named result
  to the next phase.
- Public APIs should expose stable semantic values and operations; internal
  compiler, linker, cache, evidence-reference, and scheduling storage should
  remain behind those transitions.

## Decisions

- Do not create a single global “analysis result” type. Preserve local detail
  for fact, flow, projection, and linking budgets, and adapt each once to the
  canonical report status.
- Keep the evidence boundary ordered as matching raw groups -> validated
  classification evidence -> private resolved finding evidence -> serialized
  report traces.
- Narrow public APIs in implementation order: hide internal re-exports first,
  then replace raw phase DTO fields with named accessors, then consolidate
  validation and report aggregation. This minimizes temporary compatibility
  layers.
- Keep provider, filesystem, and project-resolution ownership unchanged; these
  cross-cutting findings concern Core's internal phase and public API seams.

## Coverage

Reviewed all twelve chunk reports after their decisions were resolved, the
root and Core architecture/testing/contribution guidance, Core public
re-exports, local/frozen phase types, completion/status transitions, compiler
validation, evidence construction and report rendering, project session
transitions, and direct workspace callers. The report intentionally groups
shared roots rather than adding a thirteenth pass of local duplicate findings.

## Handoff

Chunk 13 is the final cross-cutting report. The complete audit set is now
Chunks 1–12 plus this cross-cutting architectural review.
