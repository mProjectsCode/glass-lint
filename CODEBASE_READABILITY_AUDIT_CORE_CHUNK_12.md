# Codebase Readability Audit — Chunk 12: Retained module, scope, and value models

## Summary

Read-only audit of the retained semantic domain types in `glass-lint-core`
(`analysis/model/{module,scope,scope/provenance,static_properties,value}.rs`
plus their tests), and of representative consumers in scope query, resolution,
facts, flow, matching, project session, and project linking/identities.

The models are generally well-encapsulated: `LexicalScopes`, `ScopedName`,
`ProvenanceAlternatives`/`ProvenanceJoin`, `AliasAssignment`, `StaticProperties`,
and `StaticObject` hide their storage behind narrow domain operations, and the
scope/binding model respects the `NameId`-opaque invariant (all cross-phase
interfaces use strings and `SmolStr`). The audit reports five findings:

- READ-001: the retained `ModuleInterface` carries a project-request authoring
  operation (`for_each_request` + `kind()`) that imports project types into the
  semantic model.
- READ-002: `ValueTable` exposes a `static_string` accessor but forces two
  consumers to rebuild static-object and rooted-chain extraction from raw
  `Value` matches.
- READ-003: request-existence validation is duplicated as a fetch-and-discard
  `let Some(_request) = ...` across four project-phase call sites.
- READ-004: the parallel ID newtype family drifted (an undocumented width
  split, a `Default` derive that fabricates the live program scope id on
  `ScopeId`, and per-type test constructors).
- READ-005: `intern_value_with_binding(value, None)` dominates the interning
  surface, hiding the plain `intern_value` path from 8 of 10 production calls.

Fix Applied: None so far.

## Findings

### Retained module interface and request model

#### [x] READ-001 — `ModuleInterface` imports project request types and owns project-phase request authoring

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/model/module.rs:6-11, 149-157, 352-370`

`Model::ModuleInterface` is a retained semantic model, yet `for_each_request`
(module.rs:352-370) takes `&ProjectRelativePath` and `&SourceLineIndex` and
emits `crate::project::ResolutionRequest`/`ResolutionRequestKey` values. To do
so the model imports project input types (module.rs:8-10) and owns the
`ModuleRequestRole -> ResolutionRequestKind` classification
(`ModuleRequest::kind`, module.rs:149-157). `kind()` is used only by
`for_each_request` and its unit test (`module/tests.rs:86-97`), and
`for_each_request` has exactly one production caller
(`project/session/artifacts.rs:144-151`), which stores the produced keys in the
authored-request table. This places a project-session transition (authoring
resolver keys from a source position) inside the provider-neutral retained
model, so the model can no longer be read or reused without the project request
types tag-along. Project consumers already classify on the model-owned role
directly (`ModuleRequestRole` matches at `identities.rs:119-140`,
`resolver.rs:113-117`, `export.rs:151`), so only the role-to-`ResolutionRequestKind`
mapping is the project-phase decision leaking into the model.

**Recommendation:** Replace the `for_each_request` callback with a direct loop
in `record_local` (`project/session/artifacts.rs:138-155`) over
`interface().request_entries()`, applying the `ModuleRequestRole ->
ResolutionRequestKind` mapping inline beside its only caller. Delete
`for_each_request`, `ModuleRequest::kind`, and the `crate::project` import from
module.rs — `kind()` returns a project type, so it cannot remain in the model
once its only consumers (the callback and `module/tests.rs:75-99`) move with
it; move the kind-mapping assertions to the project/session module and keep the
role-constructor assertions in `module/tests.rs`. Guardrails: the authored
`ResolutionRequestKey` values must stay byte-identical (same
`importer.clone()` per key, same role mapping, same
`lines.try_range(request.span()).ok()` skip-on-error) so `is_authored_request`
validation and resolver outcome keys are unchanged; request enumeration order
must not change; the model keeps the `role()`/`specifier()`/`span()` accessors
the replacement loop reads.

**Fix Applied:** Removed project request imports, `ModuleRequest::kind`, and
`ModuleInterface::for_each_request` from the core model. Project session
`record_local` now maps roles to request kinds and constructs authored request
keys directly in the existing source order. Added a project-session regression
test covering all five role mappings.

#### [x] READ-003 — Request "does this id exist" is re-derived by four fetch-and-discard lookups

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/resolver.rs:233-240`, `glass-lint-core/src/analysis/project/identities.rs:195-198`, `glass-lint-core/src/analysis/project/linker/export.rs:276-282, 313-319`

Four call sites validate a `ModuleRequestId` by fetching the request through
`interface().request(id)` and discarding it
(`let Some(_request) = ... else { ... }`): `walk_star_exports`
(resolver.rs:233-240, records `saw_unknown`), `collect_exported_identities`
(identities.rs:195-198, `continue`), `resolve_namespace_export` and
`resolve_request_export` (export.rs:276-282 and 313-319, return
`ExportResolution::Unknown`). Each caller rebuilds the same
module → interface → request-index existence check (`module.rs:310-312`) and
then independently decides the failure semantics, so the existence invariant of
the id domain is not owned by `ModuleInterface` and the failure handling can
drift.

**Recommendation:** Add one narrow existence operation on `ModuleInterface` —
`pub fn has_request(&self, id: ModuleRequestId) -> bool` delegating to the same
`self.requests.get(id.index())` path as `request` — replace the four discarded
fetches with it (keeping each site's outer `module()`/`modules.get()` lookup),
and keep the per-caller failure semantics (`saw_unknown` vs `Unknown` vs
`continue`) unchanged. Guardrails: do not fuse the different fallback behaviors
into a shared helper — only the existence check deduplicates; the two
`export.rs` callers must still return `ExportResolution::Unknown` when absent,
`walk_star_exports` must still record `saw_unknown`, and `identities.rs` must
still `continue`; `has_request` must not consult the project-phase
`AuthoredRequestTable` (it validates the local module interface, not authored
status).

**Fix Applied:** Added `ModuleInterface::has_request` and replaced the four
discarded request fetches while preserving each caller’s existing fallback
behavior. Added valid/invalid request-ID assertions to the module model tests.

### Value arena and static values

#### [x] READ-002 — Consumers rebuild `StaticObject`/`RootedMember` extraction that `ValueTable` should own

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/model/value.rs:280-285`, `glass-lint-core/src/analysis/flow/matcher.rs:113-126`, `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:230-235`

`ValueTable` owns value-arena accessors `static_string(id)` (value.rs:280-285)
and `binding_slot(id)` (value.rs:268-273), but the analogous `StaticObject`
and rooted-chain views are rebuilt by consumers from a raw
`match values.resolve(id) { Value::StaticObject(o) => Some(o), .. }` /
`Value::RootedMember { path } => Some(path)` split. The two known instances
are `ArgumentData for CallArgInfo` in flow/matcher.rs:113-126 (the
`static_object` and `rooted_chain` trait methods, while `static_string`
already delegates to `ValueTable`) and `argument_with_overlay` in
matching/arguments/evaluator.rs:230-235. Both do the same
resolve-then-variant-match over the same two variants, so the "what value does
this id carry" interpretation is duplicated rather than owned by the arena.

**Recommendation:** Add `ValueTable::static_object(id) -> Option<&StaticObject>`
and `ValueTable::rooted_member(id) -> Option<&NamePath>` beside `static_string`,
at the same visibility, each delegating to the existing `resolve` (so
`Value::Binding` chains keep resolving), and rewrite the two call sites to call
them. Delete the duplicated match arms. Guardrails: the accessors must keep
resolving through binding chains exactly like `static_string` does
(`resolve` → `get`), and `evaluator.rs` must preserve the fall-through
`(None, None)` for unknown/non-static values — a value can be at most one of
`StaticObject`/`RootedMember`, so the paired result is unchanged — keeping
matcher behavior and evidence order identical.

**Fix Applied:** Added `ValueTable::static_object` and
`ValueTable::rooted_member`, both resolving through binding chains, and routed
the flow matcher and argument overlay through those accessors. Added a value
table test covering static-object and rooted-member extraction.

#### [ ] READ-005 — Interning surface pushes `intern_value_with_binding(.., None)` onto 8 of 10 production calls

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/value.rs:157-208`, `glass-lint-core/src/analysis/resolution/expression/static_values.rs:102-148`

`intern_value` (value.rs:157-197) is private, so the only plain-value intern
entry visible to production callers is `intern_value_with_binding`
(value.rs:199-208), whose `binding: Option<BindingKey>` parameter wraps the
value in `Value::Binding` (`intern_static_object`, value.rs:210-228, is the
other production-facing entry but only handles object shapes and delegates to
the same function). Eight of the ten production calls pass `None`
(static_values.rs:105-146, call.rs:87, expression.rs:346); only
`intern_bounded_const_value` (resolution/constant.rs:95) and the bound-argument
path (resolution/call.rs:114) actually use a binding. Callers that never bind
must still name the two-argument entry and pass `None`, so the plain-intern
path is re-expressed at every call site instead of once.

**Recommendation:** Expose plain `intern_value` as `pub(in crate::analysis)`
and route the eight no-binding call sites through it, leaving
`intern_value_with_binding` as the explicitly-named wrapper for the two
binding-aware sites (better, rename the wrapper to make the binding intent
explicit). Guardrails: keep the capacity/pop/exhausted and terminal-cache
behavior identical for both entry points — `intern_value` already performs the
binding-terminal and `MAX_VALUES` bookkeeping (value.rs:158-194), so the exposed
entry must not change it — and do not change the `Value::Binding`
chaining used by the two binding-aware callers.

### Scope and binding identity model

#### [ ] READ-004 — Parallel ID newtypes drifted in width, sentinel, and test surface

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:16-29, 51-103`, `glass-lint-core/src/analysis/model/module.rs:114-121`, `glass-lint-core/src/analysis/model/value.rs:9-57`

The retained ID family is not consistently encapsulated. The width split is
partly structural: `ScopeId(usize)` (scope.rs:17) and `ModuleRequestId(usize)`
(module.rs:115) index unbounded `Vec` storage directly
(`LexicalScopes.0.get(scope.0)`, scope.rs:223; `self.requests.get(index.index())`,
module.rs:311), while the `u32` ids — `BindingId`, `BindingVersion`,
`FunctionId` (scope.rs:52, 66, 80) and `ResolvedObjectId`, `FlowObjectId`
(value.rs:30, 46) — either cap their arenas (`MAX_VALUES`/`MAX_OBJECTS` =
65_536, value.rs:135, 245) or satisfy the datastructures `IdIndex` contract
(`FunctionId`, scope.rs:99-103; `table.rs:18`). The split is nonetheless
undocumented, and the surrounding conventions drifted: `ScopeId` derives
`Default` (scope.rs:16), so `ScopeId::default()` yields `ScopeId(0)` — the live
program scope id (`LexicalScopes::program_scope`, scope.rs:230-232), not a
sentinel — while only `ValueId` defines an explicit `UNKNOWN` sentinel
(value.rs:13) and the other ids have no sentinel at all. Each id also
re-implements near-identical `#[cfg(test)] from_test` (and, for `ScopeId`,
`index_for_test`) constructors — `ModuleRequestId` (module.rs:114-121) has
none — so there is no single convention to copy when a new id is added.

**Recommendation:** Remove the `Default` derive from `ScopeId` (no production
or test caller uses it, and it fabricates a valid program scope), document the
width convention once at the top of `model/scope.rs` and `model/value.rs` —
`u32` for bounded-arena / `IdIndex` ids, `usize` for direct `Vec`-indexing
ids — and consolidate the repeated `from_test`/`index_for_test` test
constructors behind one shared `#[cfg(test)]` helper or macro so a new id
copies one spelling. Guardrails: no production code path may construct a
`ScopeId`/id out of thin air after the change — all ids must continue to
originate from their allocating collections (`LexicalScopes`, `BindingIndex`,
`ValueTable`, `ModuleInterface`); `ValueId::UNKNOWN` must remain index 0; do
not re-width `ScopeId`/`ModuleRequestId` to `u32`, which would add a
`usize::try_from` at every `Vec` index without a capacity bound to enforce.

## Systemic Themes

- **Adapter methods keep phase boundaries honest.** `ModuleInterface` request
  accessors, `ScopeGraph`/`FrozenScopeGraph` delegation, and the
  `const_value_to_provenance` / `provenance_to_const_value` pair are narrow,
  well-documented bridges; the exceptions are READ-001 (project types pulled
  into the model) and READ-002 (arena interpretation rebuilt at consumer sites).
- **Existence-checks are re-derived instead of owned.** Across the project
  phase, consumers answer "does this request id exist" by re-fetching the
  request through `interface().request(id)` at five sites — READ-003's four
  fetch-and-discard lookups plus `resolve_namespace` (identities.rs:228-233),
  which fetches the same way and then uses the payload — signalling that the
  request-existence invariant belongs on `ModuleInterface`, which today exposes
  it only through the fetch-a-payload accessor.
- **Newtype conventions drift within one family.** Width, sentinel, `Default`,
  and test-constructor spelling differ across the id types (READ-004). The
  width split tracks storage (`u32` for bounded arenas / `IdIndex`, `usize` for
  direct `Vec` indexes) but is undocumented; the `Default` derive on `ScopeId`
  and the per-type test constructors are pure drift, and the same
  `Default`-on-an-id derive recurs on `ControlRegionId` (fact.rs:50), where it
  is a deliberate counter seed (facts/state.rs:32) — so the family has no
  single documented rule for when `Default` is acceptable.
- **The scope and value static-shape models are deliberately parallel but
  undocumented as such.** `BindingProvenance::StaticString/StaticNumber/
  StaticStringArray/StaticObjectKeys/StaticObjectValues` (scope/provenance.rs:
  45-49) and `Value::StaticString/StaticNumber/StaticArray/StaticObject`
  (value.rs:126-129) are separate phase-local vocabularies, each with its own
  `ConstValue` adapter; this is defensible given build order but worth one
  comment naming the lifecycle split.

## Open Questions — Resolved

1. **No — the scope and value static variants should not share a shape token.**
   `BindingProvenance`'s static variants hold scope-phase data keyed by `NameId`
   and carry no value ids (`StaticObjectKeys(StaticProperties<()>)`,
   `StaticObjectValues(StaticProperties<NamePath>)`, scope/provenance.rs:48-49),
   while `Value`'s static variants reference the value arena
   (`StaticArray(Vec<ValueId>)`, `StaticObject(StaticObject)` with
   `entries: StaticProperties<ValueId>`, value.rs:65-67, 128-129). The pipeline
   builds scopes/bindings/provenance ("scopes, bindings, provenance, and
   semantic facts", core `ARCHITECTURE.md:12`) before the value arena
   ("module interfaces and bounded flow summaries", `ARCHITECTURE.md:14`);
   scope provenance is produced by `scope/build` via `const_value_to_provenance`
   (static_value.rs:16-41), which interns names but has no `ValueTable`. The
   vocabularies also differ in intent: the scope phase retains object keys only
   ("Object values are intentionally retained as keys only", static_value.rs:13-15),
   while the value phase retains the full value graph. Unifying would reparent
   construction order and force an `Option`-valued payload; the readable fix is
   the naming comment proposed under Systemic Themes.
2. **The width split is structural, not a cache-friendliness choice — document
   it, do not re-width.** The `u32` ids back bounded arenas
   (`MAX_VALUES = 65_536`, value.rs:135; `MAX_OBJECTS: u32 = 65_536`,
   value.rs:245) or satisfy the datastructures `IdIndex` contract
   (`Copy + Into<u32>`, `table.rs:18`; `FunctionId`'s impl at scope.rs:99-103,
   used by `FunctionTable<T> = IndexTable<FunctionId, T>`, flow.rs:10-11).
   `ScopeId`/`ModuleRequestId` index plain unbounded `Vec`s directly
   (scope.rs:223, module.rs:311), so `usize` avoids a conversion at every `get`.
   Forcing `u32` on them would add `usize::try_from` noise with no capacity
   bound to enforce; READ-004's actionable defects are the `Default` derive and
   the test-constructor spelling drift, not the widths.
3. **No — `to_const_object` is keys-only by design and cannot serve the value
   phase.** `to_const_object` (static_properties.rs:73-82) projects keys with
   all-`Unknown` values and is used by the scope-phase adapter
   `provenance_to_const_value` for `StaticObjectKeys`/`StaticObjectValues`
   (static_value.rs:54-59). The value-phase projection (`const_value_depth`,
   constant.rs:27-57) walks `StaticObject` entries recursively
   (`object.iter()` → `const_value_depth(value_id, ...)`, constant.rs:45-54),
   producing value-bearing `ConstValue` trees. A single projection cannot serve
   both; keep `to_const_object` as the scope-phase keys-only adapter.

## Coverage

- Definitions: `analysis/model/mod.rs`, `module.rs`, `scope.rs`,
  `scope/provenance.rs`, `static_properties.rs`, `value.rs`; tests under each
  model directory; `analysis/model/flow.rs` (for `FunctionTable`/`IdIndex`
  context).
- Consumers: `analysis/scope/{graph.rs, scope_index.rs, binding_index.rs,
  mutation_index.rs, name_env.rs, frozen_assignments.rs, static_value.rs,
  query/bindings.rs, query/functions.rs, query/provenance/{chain.rs,
  callable.rs, object.rs}`; `analysis/resolution/{mod.rs, constant.rs,
  call.rs, expression.rs, expression/static_values.rs}`; `analysis/facts/
  {mod.rs, stream.rs, arguments.rs}`; `analysis/flow/{matcher.rs,
  summary/parameter.rs, projector/driver.rs}`; `analysis/matching/arguments/
  evaluator.rs`; `analysis/module_request.rs`;
  `analysis/project/{model.rs, identities.rs, resolver.rs, linker/export.rs}`;
  `project/types/input/resolution.rs`; `project/session/artifacts.rs`.
- Documents: repository `AGENTS.md` and `ARCHITECTURE.md`,
  `glass-lint-core/ARCHITECTURE.md`, and the audit skill workflow were
  followed; `git status` confirmed no source was modified.
