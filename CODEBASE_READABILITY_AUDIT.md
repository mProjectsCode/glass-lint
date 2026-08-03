# Codebase Readability Audit

## Summary

This report replaces the previous historical audit, whose findings were all
marked fixed. It records only current, unresolved issues found in the source
as of 2026-08-03.

There are 10 findings: 1 High, 7 Medium, and 2 Low. The dominant theme is
that several types have a good semantic name but still expose a map, vector,
or field-oriented representation to neighboring modules. The clearest
consolidation opportunity is the duplicate `QualifiedEvent` type used by
cross-flow propagation and trace construction.

## Findings

### Semantic model ownership

#### [x] READ-001 — `LexicalScope` exposes its binding table

- **Severity:** High
- **Fix Complexity:** High
- **Category:** Encapsulation, Newtype, API
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:166-173`; consumers in `glass-lint-core/src/analysis/scope/build/collector.rs:74`, `scope_index.rs:47`, `binding_index.rs:106`, and `build/assignments.rs:108-505`

`LexicalScope` is a public semantic model, but its `HashMap<NameId,
BindingProvenance>` is public and is manipulated directly by builders,
indexes, and assignment analysis. Those callers perform binding insertion,
lookup, membership checks, and key iteration themselves, so the scope type
does not own the invariants or vocabulary of its binding operations.

**Recommendation:** Make the fields private and give `LexicalScope` a
validated constructor plus domain operations such as `binding`,
`contains_binding`, `insert_binding`, and a deliberately named binding
iterator. If the table needs its own policy, introduce a private
`ScopeBindings` collection and keep scope metadata separate from binding
storage. Update the builders and indexes to use those operations rather than
reconstructing map semantics at each call site.

**Fix Applied:** Made the binding table a private `ScopeBindings` collection
owned by `LexicalScope`, added the scope constructor and named operations for
insertion, lookup, membership, and binding iteration, and migrated the
planner, collector, indexes, assignment analysis, visitor, and scope tests to
those operations. The physical `HashMap` is no longer part of the neighboring
modules' API.

#### [x] READ-002 — `ExportEntry` leaks an invalid-state-prone record

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, API, Newtype
- **Location:** `glass-lint-core/src/analysis/model/module.rs:67-82`, with state updates at `module.rs:180-245`

`ExportEntry` exposes three independent `Option` fields even though it is
stored only inside `ModuleInterface` and all current construction and updates
already happen through `ModuleInterface` methods. Callers can therefore create
or mutate combinations that the interface's export-resolution policy may not
intend, while the semantic operations remain split across raw assignments.

**Recommendation:** Make `ExportEntry` private to the module or make its
fields private and expose read-only semantic queries if the type must remain
visible. Move state transitions such as “unknown export”, “function export”,
and “static string export” onto `ExportEntry` or `ModuleInterface`, so the
three optional representations cannot be changed independently by callers.

**Fix Applied:** Made `ExportEntry` and all three optional representation
fields private to `module.rs`. Added named entry constructors and state
transitions for resolutions, function exports, static strings, and unknown
exports, then migrated `ModuleInterface` updates to those operations. Callers
continue to use the interface's semantic methods without being able to create
or mutate invalid entry combinations directly.

### Cross-flow and matching containers

#### [x] READ-003 — Cross-flow state has field-level evidence transitions

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation, Complexity, Newtype
- **Location:** `glass-lint-core/src/analysis/flow/cross/state.rs:49-61`; direct use in `cross/propagation.rs:97-182` and `cross/evidence.rs:281-295`

`CrossFlowState` is the semantic state for one propagated call context, but
its source, requirement evidence, and sink evidence are exposed to every
cross-flow phase. Propagation inserts directly into `IndexedEvidence`, while
trace emission separately performs value collection, sorting, filtering, and
deduplication for prior sinks; readiness is also recomputed from raw lengths
at the call site.

**Recommendation:** Keep the state fields private and add operations named
after the state transitions, such as `record_requirement`, `record_sink`,
`requirements_ready`, `sinks_complete`, and an ordered prior-sink/event view
for trace assembly. Constructors for the “known source” and “unknown source”
alternatives should also live on the state type, reducing repeated literal
initialization in the worklist. This would make the certainty-preserving
rules visible at the owner boundary instead of encoded in callers.

**Fix Applied:** Made `CrossFlowState` evidence and source fields private and
added named constructors for known and unknown source alternatives. The state
now owns requirement/sink recording, readiness and all-sink completion checks,
source access, and deterministic prior-sink trace preparation. Propagation and
trace assembly use those transitions instead of inserting into
`IndexedEvidence`, inspecting raw lengths, or reimplementing sink ordering.

#### [x] READ-004 — `FlowSources` exposes adjacency normalization

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, Newtype, Duplication
- **Location:** `glass-lint-core/src/analysis/flow/cross/sources.rs:124-205`, `sources.rs:229-275`, and test setup in `cross/mod.rs:317-457`

`FlowSources` owns deduplicated source candidates and a normalized adjacency
index, but both maps are `pub(super)`. Production code inserts adjacency
vectors and propagates through raw map entries, while tests construct edges
with `adjacency.insert`, bypassing the type's sorting and deduplication
policy. The type therefore has a semantic contract that its callers can
silently violate.

**Recommendation:** Make both maps private. Add operations such as
`add_edge`, `destinations`, `candidates`, and a named candidate/edge iterator;
keep vector insertion, sorting, and deduplication inside `FlowSources`. Tests
should use the same edge operation so they exercise the actual domain API.

**Fix Applied:** Made the source and adjacency maps private and added named
candidate, edge, destination, and flattened-entry operations. `add_edge`
performs sorted insertion and deduplication at the ownership boundary, so
propagation and all tests use the same normalization policy instead of
writing adjacency vectors directly.

#### [x] READ-005 — Package overlay lookup still accepts raw nested buckets

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Encapsulation, Newtype, API
- **Location:** `glass-lint-core/src/analysis/matching/mod.rs:160-220`; `glass-lint-core/src/analysis/matching/occurrence.rs:238-272` and `occurrence.rs:448-456`

`LinkedOccurrenceView` correctly centralizes identity remapping, but its
package path still passes a `BTreeSet<ModuleExportKey>` and a nested
`BTreeMap<ModuleExportKey, Vec<&[Occurrence]>>` into
`OccurrenceIndex::package_candidates_with_overlay`. The receiving iterator
then stores raw map iterators and independently understands masking and base
versus overlay precedence. The remaining boundary is therefore still shaped
by storage rather than by package-occurrence behavior.

**Recommendation:** Introduce an opaque overlay view or package-candidate
source that owns masking, bucket lookup, and precedence. Have
`LinkedOccurrenceView` pass that semantic object to the occurrence iterator,
or let the view itself provide package iteration. Keep the nested bucket
layout private to the matching implementation.

**Fix Applied:** Added an opaque `PackageOverlay` that groups the mask and
linked package buckets and is consumed by the lazy package iterator. The
linked occurrence view now constructs that semantic overlay, while
`OccurrenceIndex` accepts it as one package-candidate source instead of
receiving separate raw nested storage containers.

#### [x] READ-006 — Matcher index families repeat storage plumbing

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** Encapsulation, Duplication, Simplification
- **Location:** `glass-lint-core/src/analysis/matching/indexes.rs:8-118`; projection writes in `matching/build.rs:42-116` and `build.rs:120-207`

`CallIndexes`, `MemberIndexes`, `ConstructionIndexes`, and `LiteralIndexes`
are named domain groups, but their individual occurrence indexes are exposed
as `pub(super)` fields. The fact projector reaches through those groups to
push into specific indexes, and each group repeats a long `normalize` and
test-only `is_empty` implementation. The result is a large parallel set of
physical choices at every recording site rather than one owner for recording
and normalization.

**Recommendation:** Keep the physical indexes private and give each family
small recording methods, or let `OccurrenceIndexes` own event-specific
recording methods that delegate internally. Centralize family normalization
and test inspection behind those owners. Preserve the separate indexes where
their lookup semantics differ; the simplification is the boundary and
repeated plumbing, not necessarily collapsing all indexes into one map.

**Fix Applied:** Kept the distinct physical index families but made their
storage fields private. Each family now owns event-specific recording methods,
normalization, and named read-only accessors; fact projection and query code
no longer push into or reach through individual occurrence indexes. Test-only
inspection also uses those family boundaries.

### Duplicate semantic abstractions

#### [x] READ-007 — Cross-flow duplicates trace `QualifiedEvent`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Category:** Duplication, Newtype, API
- **Location:** `glass-lint-core/src/analysis/trace.rs:9-13`; duplicate in `analysis/flow/cross/state.rs:42-47`; conversions in `flow/cross/evidence.rs:232-263`

The trace layer and cross-flow state each define the same `(ModuleId,
FactId)` value with the same ordering and identity semantics. Trace assembly
must manually convert every cross-flow event into the trace version, and the
cross-flow version repeats the trace type's public field exposure.

**Recommendation:** Consolidate on one qualified-event type at the neutral
analysis boundary, then give it named accessors such as `module()` and
`fact()` rather than making its representation the API. Update cross-flow
state, trace assembly, and tests to construct and pass that type directly.
Do not merge it with `SourceKey` or other keys whose value domain includes a
function or value identity; those represent different concepts.

**Fix Applied:** Removed the cross-flow duplicate and re-exported the neutral
trace-layer `QualifiedEvent` through the cross-flow state module for existing
internal call sites. Its fields are private behind `module()` and `fact()`;
cross-flow evidence now passes the shared value directly into trace assembly
without per-event conversion.

### Rule query boundary and construction

#### [x] READ-008 — Query/compiler code bypasses validated query accessors

- **Severity:** Medium
- **Fix Complexity:** High
- **Category:** API, Encapsulation, Architecture
- **Location:** `glass-lint-core/src/api/rule/query/mod.rs:136-144` and `mod.rs:268-276`; consumers in `api/rule/query/expression.rs:116-185`, `api/compiler/normalize.rs:342-350` and `475-497`, `api/compiler/physical.rs:491-501`, and `api/compiler/reference.rs:551-568`

The authoring query types already provide semantic accessors, but the query
normalizer, expression formatter, and compiler still reach into `pub(crate)`
fields such as `sources`, `constraints`, `var`, `event`, `identity`, and the
nested static-string predicate `kind`. This makes the compiler depend on the
construction representation and leaves canonicalization and matching logic
split between the query and compiler modules.

**Recommendation:** Make authoring fields private to the query module and
add focused semantic views or transformations, such as
`source_events()`, `argument_constraints()`, `event_identity()`, and a
predicate alternative iterator/kind accessor. Move mutations and
canonicalization into query-owned methods where practical. Keep normalized IR
field-oriented only after the authoring-to-IR boundary, so compiler code can
still be simple without exposing the declaration model's storage.

**Fix Applied:** Made the authoring storage private for event, lifecycle, and
declaration queries, query expressions, and matcher predicates. Added
semantic accessors and test-only construction helpers, then migrated query
formatting, normalization, physical planning, reference matching, validation,
and compiler fixtures to use those boundaries. Compiler code no longer depends
on the authoring structs' field layout.

#### [x] READ-009 — Bounded canonical collections duplicate policy code

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Duplication, Simplification
- **Location:** `glass-lint-core/src/api/rule/query/value.rs:57-106`

`bounded_strings` and `bounded_paths` independently assemble a vector, trim or
parse values, sort and deduplicate, reject empty collections, and enforce the
same maximum. Their different input validation is meaningful, but the shared
canonicalization and bound policy is repeated and can drift.

**Recommendation:** Extract a small domain helper for bounded canonical
collections that accepts the per-item parser/validator and the diagnostic
label. Keep `bounded_strings` and `bounded_paths` as readable named entry
points, with only their genuinely different validation expressed locally.

**Fix Applied:** Added one private `bounded_canonical_values` helper that owns
the shared parse-result collection, trimming, sorting, deduplication, empty
collection, and maximum-size policy. `bounded_strings` retains its
static-value validation, while `bounded_paths` retains checked-chain parsing,
and both now delegate the common canonicalization and bounds behavior.

#### [x] READ-010 — Lifecycle sink factories repeat target validation

- **Severity:** Low
- **Fix Complexity:** Low
- **Category:** Duplication, Simplification, Naming
- **Location:** `glass-lint-core/src/api/rule/query/lifecycle.rs:355-435`

The four public sink constructors repeat empty-name checks, `checked_chain`
parsing, and construction of either a global or rooted-member target. The
argument and any-argument forms differ only in the stored sink variant and,
for one pair, argument-index validation.

**Recommendation:** Centralize chain parsing and target construction in a
small private helper with a name that describes the target domain, then use a
single sink builder for the argument-bearing and any-argument variants. Keep
the four public constructors because their names are useful at call sites;
the simplification should be internal to their implementation.

**Fix Applied:** Replaced the four repeated sink-construction paths with one
private `build_call_sink` helper. It now owns empty-name checks, checked-chain
parsing, argument-index validation, target construction, and selection of the
indexed versus any-argument sink variant, while the public constructors retain
their descriptive global/member and argument/any-argument names.

## Systemic Themes

- Semantic ownership is strongest when storage is private (`StaticProperties`,
  `IndexedEvidence`, `FunctionEffects`, and the project tables are useful
  examples), but several neighboring modules still use `pub(super)` fields as
  an informal API.
- The same representation leak appears at multiple scales: a single binding
  map in `LexicalScope`, nested occurrence buckets in matching, and parallel
  physical index families. Newtypes alone do not improve readability when
  their callers still perform the map/set/vector operations.
- Deterministic sorting and deduplication are often correct domain policy, but
  they should be named operations on the owner rather than anonymous cleanup
  near propagation, trace assembly, or query normalization.
- The query declaration model has a clear public builder vocabulary, while
  its compiler boundary is still field-oriented. Separating authoring views
  from normalized IR would make that boundary easier to understand and
  change.

## Open Questions

- Should the single `QualifiedEvent` live in `analysis::trace`, in a neutral
  `analysis::model` module, or in another shared semantic layer? The answer
  should follow ownership rather than forcing cross-flow to depend on trace
  internals.
- Are all four matcher index families intended to remain separate physical
  structures? If so, their recording and normalization APIs should make that
  decision invisible to fact projection; if not, READ-006 may be a candidate
  for a deeper consolidation.
- Is `ExportEntry` intentionally part of the externally supported core API?
  Current usage makes it an implementation detail of `ModuleInterface`; if it
  is public by design, it needs read-only semantic accessors and documented
  invariants instead of public mutable fields.
- Which query accessors should remain available to providers, versus which
  should be compiler-only views? This affects how aggressively READ-008 can
  narrow the `pub(crate)` surface.

## Coverage

- Reviewed the current worktree and read the root and owning-crate
  `ARCHITECTURE.md` files, `TESTING.md`, and `CONTRIBUTING.md` before the
  audit.
- Scanned the current Rust source inventory: 442 Rust files and 84,316 total
  Rust lines across the workspace at audit time. The focused review covered
  semantic model types, collection wrappers, map/set/vector field access,
  repeated normalization and deduplication, duplicate key/event types, query
  boundaries, and representative call sites in core, project, provider, and
  harness crates.
- Deliberately excluded ordinary serialized/request/report DTOs, internal
  compiler IR where field-oriented access is the intended representation, and
  the two intentionally independent logical/physical reference evaluators.
  Previously checked-off findings were not repeated unless current source
  still showed a distinct unresolved boundary, as with the package overlay in
  READ-005.
- No Rust source, tests, configuration, dependencies, or generated artifacts
  were changed. This is a report-only audit; tests and `make ci` were not run.
