# Codebase Readability Audit

## Summary

This read-only re-audit covered the workspace as of 2026-08-02: 411 Rust
files and 82,489 lines, including production code, inline tests, integration
tests, and all workspace crates. The architecture has strong crate-level
boundaries, typed IDs, bounded analysis, deterministic output goals, and
explicit incomplete-analysis states. The remaining issues are concentrated at
internal coordination boundaries where semantic types are converted back into
raw maps, tuples, and positional context.

The updated semantic-ownership review found 14 open findings. The highest-value
work is to keep project identity construction inside the project/session
types, encapsulate flow-state mutation and indexes, and narrow the fact model's
representation surface. No Rust, test, configuration, dependency, or other
documentation files were modified.

## Findings

### Group 1: Project staging, identity, and reports

#### [ ] READ-001 — Project linking reconstructs identity through correlated raw maps

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/project/session/mod.rs:413-464`; `glass-lint-core/src/project/tables.rs:13-62`; `glass-lint-core/src/analysis/project/model.rs:105-166`

`LocallyAnalyzedProject::resolve` consumes `SourceTable` and `ResolutionTable`
through `into_map`, constructs `module_ids` and `request_ids` separately, and
passes five correlated maps into `ResolvedLinkInput::build`. The importer/path
alignment and qualified-request invariant are therefore maintained by callers
through raw collection mechanics rather than by the semantic project types.

**Recommendation:** Give the source/request tables an owning transition that
assigns stable module identities and qualified request identities together, or
introduce a named project-link input aggregate that validates these maps at one
boundary. Keep raw maps private to that aggregate and preserve sorted path
identity, partial parse handling, duplicate rejection, and resolver validation.

**Fix Applied:** None so far.

#### [x] READ-002 — Report ordering is an ad hoc tuple contract outside report types

- **Severity:** Low
- **Fix Complexity:** Medium
- **Category:** API
- **Location:** `glass-lint-core/src/project/report/mod.rs:79-96`; `glass-lint-core/src/project/types/report/diagnostic.rs:41-99`; `glass-lint-core/src/analysis/project/linker/mod.rs:115-133`

`AnalysisReport::combine` and the linker each spell out their own ordering
tuples for files or diagnostics, while `FileReport` and `Diagnostic` expose
only individual fields. The ordering is part of the deterministic report
contract, but its definition is duplicated and can drift when a diagnostic
variant or location field changes.

**Recommendation:** Put the canonical comparison/key operation on the report
value types, or introduce a private report-ordering policy owned by the report
module. Reuse it for linking, aggregation, and rendering preparation, and
retain the current deterministic tie-breakers without exposing storage details.

**Fix Applied:** Added owner-facing ordering keys to `FileReport`, `Diagnostic`,
and `AnalysisDiagnostic`, then reused them from report combination and linker
canonicalization. The deterministic tie-breakers are unchanged, but tuple
construction is now centralized with the values whose fields define ordering.

#### [x] READ-003 — Scope collection uses anonymous composite identity records

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/scope/build/mod.rs:71-86`; `glass-lint-core/src/analysis/scope/build/assignments.rs:45-67`; `glass-lint-core/src/analysis/scope/build/callbacks.rs:115-140`

The scope collector uses raw `(ScopeId, NameId)` keys for function scopes and
version counters, and anonymous tuples for pending function names and call
records, even though `ScopedName` already represents the scope/name identity.
Consumers must remember tuple position and which parts are identity versus
provenance, making future fields easy to misassociate.

**Recommendation:** Use `ScopedName` wherever the pair is the key and add
named private records for calls and pending function bindings. Keep conversion
at the AST/collection boundary, preserving source order, path-local versions,
and the existing conservative callback behavior.

**Fix Applied:** Replaced scope/name tuple keys with `ScopedName` in function
bindings and version counters, and introduced named `FunctionBinding`,
`FunctionCall`, and `PendingFunctionName` records for the remaining collector
state. The visitor, callback projection, and freeze stages now use named
fields while preserving source order, path-local versions, and conservative
callback handling.

#### [ ] READ-004 — Fact model has a broad private-interface escape hatch

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/model/fact.rs:1,308-423`; `glass-lint-core/src/analysis/mod.rs:13-27`

`fact.rs` begins with `#![allow(private_interfaces)]` and exposes many public
fields on `CallArgInfo`, `ArgumentView`, `ParameterBinding`, `CallUnwrap`, and
`FactPayload`. This lets unrelated analysis code construct or depend on raw
fact representation, weakening the stated boundary that fact construction
and retained semantic artifacts are owned by the facts/lowering pipeline.

**Recommendation:** Narrow visibility to the smallest analysis modules that
need each type and replace broad field access with focused constructors,
accessors, or semantic pattern methods. Remove the module-wide allowance in
stages, keeping AST/building details private and validating the intended
provider-neutral interfaces through existing public-surface tests.

**Fix Applied:** None so far.

### Group 2: Flow state and matcher coordination

#### [x] READ-005 — Flow-state mutation is split between the table and raw-map history

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:51-141,291-334`; `glass-lint-core/src/analysis/flow/projector/history.rs:77-180`

`FlowStateTable` owns aliases, reverse object references, and lifecycle states,
but `MutationLog::transition` and free `increment_ref`/`decrement_ref`
functions receive and mutate the underlying `BTreeMap`s directly. The object
range bounds are also duplicated in `states_for` and `remove_states_for`, so
rollback, reverse-reference accounting, and state indexing are not expressed
by one owner.

**Recommendation:** Add a private object-scoped range helper or state index,
and put alias/reference transitions behind `FlowStateTable` or an
`ObjectRefCounts` semantic type. Let history replay owner-facing deltas while
retaining deterministic ordering, rollback behavior, and mutation/state
budgets.

**Fix Applied:** Added the `ObjectRefCounts` semantic type for alias reference
accounting and routed both live mutations and history replay through it. A
single object-range helper now owns the `FlowStateKey` bounds used by state
queries and removal, reducing duplicated indexing logic while retaining
deterministic rollback and mutation-budget behavior.

#### [x] READ-006 — Pending flow finalization carries discarded positional identity

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:196-208,666-687`

`PendingFlowStates` uses `(usize, FlowId, FactId)` as a key and stores
`(usize, FlowState)` values. Finalization destructures rule, flow, and event,
uses only the event and path index, and ends with `let _ = (rule, flow, fact)`,
while `FlowState` already carries flow identity. This makes the actual grouping
invariant difficult to see and leaves redundant state in the queue.

**Recommendation:** Introduce named `PendingFlowKey` and `PendingState`
types, then remove redundant components or give each retained component a
meaningful use in finalization. Preserve active-path certainty, fact event
locations, and deterministic queue order while making key/state divergence
visible to the type checker and reviewers.

**Fix Applied:** Replaced the pending-flow tuple key and value with named
`PendingFlowKey` and `PendingState` types. The key now retains only the flow
and event it groups, while the state names its path index; finalization uses
those fields directly and no longer carries or discards redundant rule, flow,
or fact parameters.

#### [ ] READ-007 — Projection coordinators pass mixed context positionally

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/project/projection.rs:131-174,320-368`; `glass-lint-core/src/analysis/matching/arguments/mod.rs:55-130`

`project_facts` receives nine positional arguments, including several optional
identity and overlay contexts. The constrained matcher has a related large
coordinator with lint suppressions for argument count and line count while it
prepares evaluators, scans indexed/fallback candidates, and emits evidence in
one control path.

**Recommendation:** Introduce named `ProjectionInputs` and
`MatcherEvaluationContext` values for stable context, while keeping mutable
evidence, trace arenas, and budgets explicit. Split preparation, indexed scan,
fallback scan, and emission into owner-oriented phases without changing
fail-closed unknown handling, deterministic evidence, or budget accounting.

**Fix Applied:** None so far.

#### [x] READ-008 — Flow environment exposes duplicate reachability APIs

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Naming
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:477-487`; callers in `glass-lint-core/src/analysis/flow/projector/mod.rs:356,583,633`

`FlowEnvironment::is_reachable` and `FlowEnvironment::reachable` return the
same field and are both used in production code. The duplicate API makes it
unclear whether one name is intended to carry different semantics and adds an
unnecessary internal compatibility surface.

**Recommendation:** Keep the boolean-predicate form `is_reachable` and update
all callers. Remove the forwarding alias unless a real external stability
requirement is identified, since this type is private to analysis.

**Fix Applied:** Removed the duplicate `reachable` accessor and updated both
flow projection call sites to use the predicate-named `is_reachable` method.
The private flow environment now has one reachability API, so its semantics
cannot drift between aliases.

#### [x] READ-009 — Production flow outcomes retain suppressed dead APIs

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:50-68,799-818`; `glass-lint-core/src/analysis/facts/origin_map.rs:133-141`

`LocalFlowProjectionOutcome::objects_used` is populated and marked
`dead_code`, but production projection does not consume it; it is currently
useful only to tests. `OriginMap::len` and `OriginMap::is_empty` have the same
individual dead-code suppressions, leaving stale API surface in bounded hot
path abstractions.

**Recommendation:** Remove unused APIs or make test-only introspection
`#[cfg(test)]`. If object allocation is intended as profiling data, carry it
through the owning outcome to a real consumer; keep any retained allowance
local and document its purpose.

**Fix Applied:** Removed the unused `objects_used` field from the production
flow outcome and deleted its test-only assertion, along with the unused
`OriginMap::len` and `OriginMap::is_empty` methods. The bounded flow and
origin-map abstractions now expose only data consumed by production code or
their actual invariants.

### Group 3: Project loading and harness boundaries

#### [ ] READ-010 — Project loader module combines several phase state machines

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-project/src/loader.rs:1-866`

The loader file contains public outcomes and timings, deadline and budget
coordination, path work queues, resolution caches, wave outcomes, project load
state, frontier closure, and final report assembly. `ProjectLoadState` is a
reasonable coordinator, but unrelated admission, resolution, metrics, and
finalization changes still require navigating one large module.

**Recommendation:** Extract metrics/timing, frontier management, resolution
caching, and finish/report assembly behind cohesive private modules or domain
types while retaining `ProjectLoader` as the facade. Preserve typed phase
outcomes, deterministic partial results, and all current admission, resolver,
deadline, and source-byte bounds.

**Fix Applied:** None so far.

#### [x] READ-011 — Resolution cache key hides normalization and cache identity

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Newtype
- **Location:** `glass-lint-project/src/loader.rs:417-462`

`ResolutionCache::by_specifier` uses the anonymous triple
`(ProjectRelativePath, ResolutionRequestKind, String)`, with request-to-string
conversion performed inline in `specifier_key`. The occurrence cache and
semantic cache have different invariants, but their semantic identity is not
represented by a named type.

**Recommendation:** Add a private `ResolutionSpecifierKey` with named fields
and a constructor that owns normalization/conversion. Keep the two caches
separate and make additions such as resolver conditions or mode changes happen
through the key's invariant rather than through repeated tuple edits.

**Fix Applied:** Added the private `ResolutionSpecifierKey` value object with
named importer, request-kind, and normalized-specifier fields. Its constructor
now owns conversion from a resolution request, while the occurrence and
semantic caches remain separate and retain their existing lookup behavior.

#### [ ] READ-012 — Harness types mix fixture authoring, protocol DTOs, and reports

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Architecture
- **Location:** `glass-lint-harness/src/types/mod.rs:1-858`

One module defines case and expectation data, conversion errors, adapter
request/response DTOs and serde conversion, finding validation, and suite
result reports. These areas have different change drivers and visibility
needs, so protocol changes and fixture-authoring changes share a large
namespace and review surface.

**Recommendation:** Split the implementation into cohesive private modules
such as `case`, `expectation`, `protocol`, and `report`, re-exporting only the
existing public surface from `types/mod.rs`. Keep protocol validation beside
DTO conversion and result aggregation beside report types, preserving the
wire schema and adapter error behavior.

**Fix Applied:** None so far.

### Group 4: Documentation and tests

#### [x] READ-013 — Output crate ownership is missing from architecture documentation

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Architecture
- **Location:** `Cargo.toml:2-11`; `glass-lint-output/src/lib.rs:1-10`; `ARCHITECTURE.md:6-43`; `glass-lint-cli/ARCHITECTURE.md:1-18`

The workspace contains `glass-lint-output`, and the CLI depends on it, but the
root crate graph and boundary table do not list it and the crate has no own
architecture document. The ownership of terminal presentation and its
relationship to core reports is therefore implicit despite crate boundaries
being a central repository rule.

**Recommendation:** Add the output crate to the root graph and boundary table,
and document that it owns presentation over core reports without absorbing
provider policy, loading, or semantic report construction. Alternatively,
explicitly state in the CLI architecture document why the output crate is
intentionally covered there.

**Fix Applied:** Added `glass-lint-output` to the workspace architecture graph
and boundary table, documented its presentation-only ownership in a new crate
architecture file, and clarified the CLI architecture boundary for format
dispatch and JSON output. The documentation now distinguishes reusable report
rendering from analysis, loading, provider policy, and report construction.

#### [x] READ-014 — Panic-safety test table obscures constructor coverage

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Testing
- **Location:** `glass-lint-core/tests/integration/query/composition.rs:181-405`

The panic-safety test is valuable, but roughly two dozen boxed closures return
`Result<(), String>`, repeat the same error-to-string adapter, and require
`too_many_lines` and `type_complexity` suppressions. Converting typed
constructor errors to strings at each table entry also makes failures less
diagnostic.

**Recommendation:** Keep catch-unwind coverage, but use a small assertion
helper or macro with a named case and retain typed errors until the assertion
boundary. Split the table by constructor family if necessary so new invalid
input cases remain easy to add without reducing adversarial coverage.

**Fix Applied:** Replaced the repeated closure/error-string adapters with a
typed `InvalidConstructor` alias, an `invalid_case!` table macro, and a named
panic-safety assertion helper. Constructor errors remain `QueryBuildError`
values until the assertion boundary, while every existing invalid-input case
keeps its descriptive name and catch-unwind coverage.

## Systemic Themes

- Semantic newtypes and opaque IDs are common and valuable, but coordination
  boundaries still repeatedly reconstruct identity from raw maps, tuple keys,
  and positional arguments.
- Ownership is clearest between crates; within large modules, mutation,
  metrics, protocol DTOs, report assembly, and cache policy are often colocated
  enough to obscure the narrow owner of a change.
- Determinism and bounded fail-closed analysis are strong architectural
  themes. Their maintenance cost would be lower if ordering, budgets, and
  state transitions were represented by owner-facing domain operations.
- No production `Rc` or `RefCell` overuse hotspot was found. The observed
  `Arc`, `Mutex`, and `OnceLock` uses are generally tied to shared caches,
  immutable artifacts, or lazy derived data rather than compensating for
  unclear ownership.

## Decisions

- **Project identity:** Introduce a private named project-link aggregate.
  `SourceTable` should own source-to-module identity, `AuthoredRequestTable`
  should construct qualified requests from supplied module IDs, and the
  aggregate should validate and combine those results without exposing raw
  maps.
- **Fact visibility:** Treat the fact model as an internal implementation, not
  a future public contract. Narrow fields to the smallest analysis scope,
  provide semantic constructors/accessors, and remove the broad
  `private_interfaces` allowance.
- **`objects_used`:** Remove it from production outcomes or make it test-only.
  Promote it to profiling metrics only if a real consumer and stable meaning
  are established.
- **Output architecture:** Give `glass-lint-output` a short standalone
  architecture contract and list it in the root crate graph. It owns
  presentation over core reports, not analysis, provider policy, loading, or
  report construction.

## Open Questions

No unresolved questions remain for the four topics above. Implementation should
follow the decisions recorded in the preceding section.

## Coverage

The review inspected all 411 Rust files in the workspace (approximately
82,489 lines), including core analysis/model/flow/matching code, project
loading and resolution, provider crates, harness and CLI crates, output, data
structures, unit tests, integration tests, and existing architecture
documents. It read `ARCHITECTURE.md`, every owning-crate architecture file,
`TESTING.md`, `CONTRIBUTING.md`, the supplied agent guide, and the updated
skill; searched semantic IDs/newtypes, raw collection escape hatches, tuple
keys, large modules/functions, public-interface allowances, dead-code markers,
ordering logic, and `Rc`/`RefCell` usage; and followed representative callers
for each finding.

Only `CODEBASE_READABILITY_AUDIT.md` was created or updated by this audit.
The pre-existing user changes in
`glass-lint-core/src/project/report/mod.rs` and
`glass-lint-core/src/project/session/mod.rs` were preserved.
