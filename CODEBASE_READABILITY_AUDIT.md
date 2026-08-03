# Codebase Readability Audit

## Summary

This report replaces the previous audit and records unresolved readability and
maintainability issues in the current worktree as of 2026-08-03. The previous
report's ten checked-off findings were treated as historical context; this
review confirms those changes where relevant and resurfaces the remaining
boundaries that were not addressed.

There are 13 findings: 3 High, 8 Medium, and 2 Low. The main pattern is that
semantic types now often hide their immediate storage, but their owning
aggregates still return storage-shaped views or let callers reconstruct domain
operations from indexes, `enumerate`, raw fields, and repeated sorting. The
clearest potential consolidation is shared lifecycle-evidence behavior for
local and cross-file flow, but it should be introduced only when that behavior
is next changed. Several similarly named keys remain intentionally distinct
because they identify different domains.

## Findings

### Project linking and semantic aggregates

#### [ ] READ-001 — Linked project aggregates expose their module and resolution maps

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Encapsulation, Newtype, Architecture
- **Location:** `glass-lint-core/src/analysis/project/model.rs:102-108, 189-212`; consumers in `analysis/project/linker/{export,mod}.rs`, `analysis/project/{identities,resolver,exports,projection}.rs`

`ResolvedLinkInput` exposes its `BTreeMap<ModuleId, ProjectModule>` and
`BTreeMap<QualifiedRequestId, LinkedModuleTarget>` to the linking transition,
and `ProjectSemanticModel` plus `ProjectLinker` expose the same physical maps
to neighboring project modules. Those callers repeatedly perform map lookup,
iteration, key construction, and resolution selection themselves, so module
ownership and resolution invariants are distributed across linker, resolver,
identity, and projection code.

**Recommendation:** Make the aggregate storage private and introduce only the
observed domain operations, such as `module`, `modules`, and
`resolution_for`, plus a linker-owned transition that consumes validated
collections without exposing their maps. Do not add `ProjectModules` or
`ProjectResolutions` merely to wrap the current maps. Add a separate collection
only if an independently repeated invariant or policy appears; keep export
fixed-point updates on `ExportTable` and cross-module resolution decisions on
the project model or a named resolver coordinator.

**Fix Applied:** None so far.

#### [ ] READ-002 — Export identity propagation still consumes a map-shaped module export view

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, Newtype, Architecture
- **Location:** `glass-lint-core/src/analysis/project/state.rs:164-179, 241-245`; `glass-lint-core/src/analysis/project/identities.rs:191-203`

`ModuleExports` is a semantic collection owned by `ExportTable`, but its
generic `get`, `insert`, and `iter` API makes identity propagation responsible
for turning `(name, ExportResolution)` pairs into `ModuleExportKey` values and
for deciding how direct exports are copied into a `ModuleIdentityMap`. The
caller therefore knows both the export-table representation and the matching
identity representation.

**Recommendation:** Add an operation named for the domain transformation,
such as `copy_identities_into(prefix, identities)`, or expose a dedicated
iterator whose item is an export identity rather than a raw map pair. Keep
monotone update and export-count accounting inside `ExportTable`; do not merge
`ModuleExportKey` with `QualifiedExportId`, because one identifies an external
module/export spelling while the other identifies an internal `ModuleId`.

**Fix Applied:** None so far.

### Scope and semantic model boundaries

#### [ ] READ-003 — `LexicalScope` still exposes structural storage metadata

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Encapsulation, Newtype, API
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:167-223`; direct consumers in `analysis/scope/{scope_index,build/{callbacks,assignments,visitor,bindings}}.rs`

The binding table was moved behind named operations, but `LexicalScope` still
publishes `span`, `depth`, `kind`, and `parent` as independent fields. Scope
indexing and collection code repeatedly indexes a `Vec<LexicalScope>` with
`ScopeId`, reads those fields directly, and separately implements parent,
containment, ordering, and function-scope decisions. The type has a semantic
name but does not own the basic vocabulary of its structural queries.

**Recommendation:** Make the metadata private and expose accessors or
predicates such as `parent`, `kind`, `span`, `contains`, `is_function_scope`,
and `depth`. Move the `Vec<LexicalScope>` behind an owning scope collection
with `get(ScopeId)`/iteration methods, so callers do not combine raw vector
indexing with scope invariants. Preserve the existing binding operations and
keep parser-specific `Span` details at the scope-analysis boundary.

**Fix Applied:** None so far.

#### [ ] READ-004 — Scope-shape storage remains public to the crate despite an existing domain API

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, Newtype
- **Location:** `glass-lint-core/src/analysis/scope/build/shape.rs:7-18`

`ScopeShapeTable` already owns recording, child consumption, exhaustion, and
test inspection, but its `shapes` vector and keyed `children` map are still
`pub(crate)`. This leaves a future caller free to append shapes or mutate
child queues without maintaining the matching key and consumption policy,
while the existing methods demonstrate that no storage access is necessary.

**Recommendation:** Make `ScopeShape` fields and `ScopeShapeTable` storage
private. Add any missing semantic read operation needed by the planner, and
retain `record`, `take_child`, and `is_consumed` as the only production
mutation boundary. Avoid replacing the table with a generic map; its ordered
child-claim behavior is the useful domain abstraction.

**Fix Applied:** None so far.

### Lifecycle and flow state

#### [ ] READ-005 — Local and cross-file lifecycle evidence duplicate the same state abstraction

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Duplication, Newtype, Simplification
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:252-453`; `glass-lint-core/src/analysis/flow/cross/state.rs:44-119`

`FlowState` and `CrossFlowState` both own requirement and sink
`IndexedEvidence`, expose nearly identical recording/readiness/iteration
methods, and encode the same lifecycle completion rules. The local version
uses `FactId` while the cross-file version uses `QualifiedEvent` and has an
optional source, but the evidence state and its operations are duplicated;
future changes to ordering, removal, or completion semantics can diverge.

**Recommendation:** When lifecycle evidence is next changed, extract one
private `LifecycleEvidence<Event>` (or similarly small domain type) that owns
the behavior actually shared by both implementations: requirement/sink
recording, indexed event access, ordered event extraction, and completion
checks. Do not introduce this layer solely because the structs look similar.
Keep `FlowState` and `CrossFlowState` as distinct owners of their
object/source identity and certainty semantics, and do not erase the
local-versus-qualified event distinction.

**Fix Applied:** None so far.

#### [ ] READ-006 — Requirement and sink indexes are rebuilt with positional enumeration at every projector

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Newtype, Encapsulation, Simplification
- **Location:** `glass-lint-core/src/analysis/flow/cross/propagation.rs:88-169`; `glass-lint-core/src/analysis/flow/projector/evidence.rs:40-141`; compiled flow storage in `glass-lint-core/src/api/compiler/object_flow.rs:32-38`

The compiled flow stores requirements and sinks as vectors, but projection
code repeatedly calls `iter().enumerate()`, converts positions into
`RequirementIndex`/`SinkIndex`, and independently interprets argument and
completion variants. This makes the positional alignment between declarations,
pre-bound paths, and evidence indexes a caller responsibility and obscures
which operations are supported by the flow plan.

**Recommendation:** Put indexed domain iteration and matching helpers on
`CompiledObjectFlow` or `BoundFlowPlan`, such as
`requirements_with_indices`, `matching_requirement_indices`, and
`matching_sink_indices`. Let those methods own the conversion to typed
indexes and the interpretation of `Any` versus explicit sink arguments, while
leaving the physical vectors private at the authoring/compiled boundary where
possible.

**Fix Applied:** None so far.

#### [ ] READ-007 — `CallContext` is a crate-visible field bag for cross-file propagation state

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, Newtype, Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/state.rs:122-130`; consumers in `cross/{evidence,mod,propagation,worklist}.rs`

`CallContext` exposes module, function, parameter, source root, state, and the
crossed flag to every cross-flow phase. Evidence and propagation code then
repeat the rules for matching a parameter root versus a source-root value and
for deciding whether a target call counts as crossed, while worklist seeding
constructs the same record literals in several forms.

**Recommendation:** Make the fields private and provide named constructors
for source-root and parameter contexts plus semantic operations such as
`module`, `function`, `state`, `matches_argument`, `is_crossed`, and
`for_target_call`. Keep `CallContext` as the owner of context identity and
move the repeated connection predicate there; do not put effect-specific
matching logic on the context.

**Fix Applied:** None so far.

#### [ ] READ-008 — Cross-flow source keys and candidates leak their field representation

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, Newtype, API
- **Location:** `glass-lint-core/src/analysis/flow/cross/sources.rs:21-64`; consumers in `cross/{sources,worklist}.rs` and `flow/projector/transfer.rs`

`SourceKey` and `SourceCandidate` are named semantic records, but all fields
are `pub(super)` and callers directly assemble, destructure, and compare their
module/function/value and flow/fact fields. `FlowSources::candidate_entries`
then flattens the nested source map into raw key/value pairs for worklist
seeding, exposing the storage traversal instead of expressing “all candidates
for propagation.”

**Recommendation:** Make the record fields private with constructors and
accessors, then add operations such as `source_identity`, `candidate_flow`,
`candidate_event`, and a named propagation-entry iterator. Keep
`FlowSources` responsible for candidate deduplication and adjacency traversal;
callers should not need to know that candidates are stored in a map of sets.

**Fix Applied:** None so far.

#### [ ] READ-009 — Bound flow planning exposes raw candidate arguments and a generic target index

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, Newtype, Simplification
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:47-95`; consumers in `flow/projector/transfer.rs:70-85` and `flow/cross/sources.rs:166-180`

`BoundTargetIndex<T>` is a generic map wrapper whose public operations are
`insert`, `get`, and `normalize`, while `BoundSource` exposes its flow and
argument vector. Source transfer therefore iterates `candidate.arguments` and
performs predicate matching outside the bound-flow plan, and source collection
must remember to normalize separate source and sink indexes.

**Recommendation:** Keep the physical target buckets private and expose
domain-specific source/sink lookup and candidate-matching operations from
`BoundFlowPlan`. Replace `BoundSource` field access with methods that express
its role, such as `flow_id` and `matches_arguments`; centralize sorting and
deduplication in the specialized index owner. Retain one generic internal
helper only if it remains completely behind the flow-plan boundary.

**Fix Applied:** None so far.

### Typed keys and representation leaks

#### [ ] READ-010 — `QualifiedRequestId` and `FlowStateKey` publish key fields and invite raw reconstruction

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** API, Newtype, Naming
- **Location:** `glass-lint-core/src/analysis/project/model.rs:45-49`; `glass-lint-core/src/analysis/model/flow.rs:371-374`; uses in `analysis/project/{identities,resolver,linker}.rs` and `analysis/flow/projector/{state,evidence}.rs`

Both types are semantic map keys with public fields, so callers repeatedly
write struct literals and use `key.module`, `key.request`, `key.object`, and
`key.flow` directly. The names are useful, but their public record shape
couples every key consumer to storage-oriented field access and makes future
key validation or alternate representation changes unnecessarily broad.

**Recommendation:** Make fields private, add constructors and accessors with
domain names, and provide owner methods such as `request_key(module, request)`
and `state_for(object, flow)` where callers currently create keys only to
perform a lookup. Keep the two key types separate: request identity and live
object-flow state are not the same domain and should remain distinct.

**Fix Applied:** None so far.

#### [ ] READ-011 — `EffectCallId` and `TraceNodeId` still expose raw numeric identity inside core

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Newtype, API
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:41-42, 348-356`; `glass-lint-core/src/api/classification.rs:13`; `glass-lint-core/src/analysis/trace.rs:75-104`

These types communicate domain identity but retain tuple-field access for raw
indexing: `FunctionEffect::call_argument` uses `call_id.0`, and
`TraceArena` constructs and indexes `TraceNodeId` directly. The leak is small
today, but it makes numeric storage part of the crate-level API and weakens
the distinction between an effect call identity, a trace node identity, and a
plain vector position.

**Recommendation:** Make the tuple fields private and add owner-scoped
conversion methods such as `index()` or `TraceArena::node(id)`. Keep raw
construction private to the allocating owner and use checked conversion at
the boundary; tests can use explicit test constructors rather than making the
production representation available.

**Fix Applied:** None so far.

#### [ ] READ-012 — `ScopeShape` and `LexicalScope` duplicate structural records without a shared vocabulary

- **Severity:** Low
- **Fix Complexity:** Medium
- **Category:** Duplication, Naming, Newtype
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:167-175`; `glass-lint-core/src/analysis/scope/build/shape.rs:7-26`

`LexicalScope` and `ScopeShape` both carry scope kind, source span, and parent
identity, while the shape table adds a second scope identifier and the scope
index reconstructs ordering from the same fields. They serve different
phases—planned shape versus collected semantic scope—so merging the structs
outright would lose useful phase meaning, but the duplicated structural
vocabulary makes ownership and conversion harder to follow.

**Recommendation:** Keep the phase types separate and do not introduce a
shared metadata struct now. Give each type semantic constructors/accessors and
make explicit which fields are planning identity versus final scope identity.
Add a private conversion helper only if a concrete repeated conversion is
introduced; do not consolidate the types merely to remove a few fields.

**Fix Applied:** None so far.

#### [ ] READ-013 — Prior-sink ordering and deduplication is duplicated outside the evidence owner

- **Severity:** Medium
- **Fix Complexity:** Low
- **Category:** Duplication, Simplification, Newtype
- **Location:** `glass-lint-core/src/analysis/flow/cross/state.rs:109-119`; `glass-lint-core/src/analysis/flow/projector/evidence.rs:330-345`

Both local and cross-file trace construction flatten evidence values into a
temporary vector, sort it, deduplicate it, and exclude the completing sink.
This is deterministic policy, not incidental iterator cleanup, and keeping it
at two call sites risks different evidence ordering or exclusion behavior as
multi-sink flows evolve.

**Recommendation:** When the shared lifecycle-evidence type in READ-005 is
actually introduced, put an operation such as
`prior_sink_events(completing_event)` on it, returning the canonical ordered
unique events. Until then, keep the normalization policy in the narrowest
existing evidence owner that is being changed. Keep trace assembly responsible
only for assigning roles and interning nodes; do not expose
`IndexedEvidence::values()` to make each trace builder repeat the same
normalization.

**Fix Applied:** None so far.

## Systemic Themes

- The repository has made meaningful progress toward private storage: the
  current `ScopeBindings`, `StaticObject`, `OriginMap`, `OccurrenceIndex`,
  `ExportTable`, and `FlowStateTable` generally keep their physical maps or
  vectors private. The remaining problems are mostly one layer above those
  wrappers, where aggregate fields and generic iterators still leak the
  representation.
- Positional identity is reconstructed in several flow paths. Typed
  `RequirementIndex` and `SinkIndex` are valuable, but their repeated creation
  from vector positions means the compiled flow—not the projector—does not yet
  own the declaration-to-evidence alignment.
- The strongest simplification is consolidation of shared behavior, not
  collapsing all similarly shaped types. `FlowState`/`CrossFlowState` are the
  candidate for shared lifecycle evidence when that code next changes;
  `QualifiedRequestId`, `FlowStateKey`,
  `ModuleExportKey`, and `QualifiedExportId` should remain distinct because
  their identity domains differ.
- Generic method names such as `iter`, `get`, `insert`, and `normalize` are
  appropriate inside a private implementation, but they become less readable
  when returned by a domain type and immediately followed by key construction,
  positional indexing, sorting, or deduplication at the caller.
- The report does not recommend replacing deterministic collections with
  compact code, removing explanatory comments, or merging logical and
  physical reference evaluators. Those are either deliberate boundaries or
  explicit architecture choices; the findings target operations that have a
  narrower semantic owner.

## Decisions

The following decisions close the questions raised by this audit:

- **Use existing semantic owners first.** `ProjectSemanticModel`,
  `ExportTable`, `FlowState`/`CrossFlowState`, `CompiledObjectFlow`, and the
  scope types should own their domain operations. Do not add aggregate
  collection newtypes or coordinator layers unless a concrete invariant is
  repeated independently.
- **Prefer private storage and the smallest observed API.** Make fields,
  vectors, and maps private where callers currently reconstruct operations from
  representation. Add only constructors, accessors, or semantic methods
  required by current call sites; do not add broad iterator, conversion, or
  compatibility APIs for hypothetical users.
- **Consolidate behavior, not identity.** Keep request, export, object-flow,
  trace, planning-scope, and collected-scope types distinct even when their
  fields resemble one another. The one credible shared abstraction is
  lifecycle evidence, and it should be extracted only as part of an actual
  lifecycle change, not speculatively.
- **Keep lifecycle and provider boundaries intact.** Any eventual shared
  evidence helper belongs in provider-neutral core flow/model code and must
  remain generic over the event type. It must not absorb provider policy,
  trace interning, or local-versus-qualified identity decisions.
- **Narrow key APIs without guessing external consumers.** For
  `QualifiedRequestId`, `FlowStateKey`, `EffectCallId`, and `TraceNodeId`, make
  representation fields private and retain only owner-scoped construction or
  read access that existing callers need. Keep a type's current visibility
  unchanged until a public-consumer audit proves it can be narrowed; do not
  add wrappers to preserve raw field access.
- **Make changes in focused slices.** Address a finding only when its owning
  code is being changed, migrate all callers in that slice, and remove the old
  representation path. No source change is implied by this report alone.

## Coverage

- Reviewed the current worktree, root `ARCHITECTURE.md`, every owning-crate
  architecture document, `TESTING.md`, `CONTRIBUTING.md`, and the previous
  `CODEBASE_READABILITY_AUDIT.md` before scanning source.
- Scanned all 442 Rust files and 84,632 Rust lines in the workspace. The
  focused review covered semantic model structs, typed IDs and keys, map/set/
  vector fields, `into_*` and iterator APIs, positional index construction,
  repeated sorting/deduplication, cross-file flow state, project linking,
  scope planning, and provider/project/harness boundaries.
- Confirmed the previous checked-off findings were not blindly repeated:
  binding storage, export-entry fields, cross-flow evidence storage, source
  adjacency normalization, package overlays, matcher index storage, the
  shared `QualifiedEvent`, authoring query fields, bounded query collections,
  and lifecycle sink construction now have the encapsulation described by that
  report. The findings above are the remaining or newly visible adjacent
  boundaries.
- Deliberately excluded serialized protocol/report DTOs, provider manifests,
  normalized compiler IR where field-oriented access is the intended private
  representation, and the intentionally separate logical/physical reference
  evaluators. Similar-looking key types were reported only when their fields
  or operations leak; intentional domain distinctions are recorded under
  Systemic Themes and Decisions.
- No Rust source, tests, configuration, dependencies, or generated artifacts
  were changed. This is a report-only audit; tests and `make ci` were not run.
