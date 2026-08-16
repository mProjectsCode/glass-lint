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
- READ-004: the parallel ID newtype family drifted (usize vs u32 widths, a
  `Default` derive that yields a live scope id on `ScopeId`, per-type test
  constructors).
- READ-005: `intern_value_with_binding(value, None)` dominates the interning
  surface, hiding the plain `intern_value` path from 9 of 11 production calls.

Fix Applied: None so far.

## Findings

### Retained module interface and request model

#### [ ] READ-001 — `ModuleInterface` imports project request types and owns project-phase request authoring

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/model/module.rs:6-11, 149-157, 352-370`

`Model::ModuleInterface` is a retained semantic model, yet `for_each_request`
(module.rs:352-370) takes `&ProjectRelativePath` and `&SourceLineIndex` and
emits `crate::project::ResolutionRequest`/`ResolutionRequestKey` values. To do
so the model imports project input types (module.rs:8-11) and owns the
`ModuleRequestRole -> ResolutionRequestKind` classification
(`ModuleRequest::kind`, module.rs:149-157). `kind()` is used only by
`for_each_request` and its unit test (`module/tests.rs:86`), and
`for_each_request` has exactly one production caller
(`project/session/artifacts.rs:144-151`), which stores the produced keys in the
authored-request table. This places a project-session transition (authoring
resolver keys from a source position) inside the provider-neutral retained
model, so the model can no longer be read or reused without the project request
types tag-along.

**Recommendation:** Move `for_each_request` (and the role-to-kind mapping it
needs) into `project/session/artifacts.rs` beside `record_local`, walking
`interface().request_entries()` and reusing the existing span-normalization
skip-on-error behavior. Delete `for_each_request` and the `crate::project`
import from module.rs, and keep `ModuleRequest.kind()` only if a non-project
consumer remains. Guardrails: the authored `ResolutionRequestKey` values must
stay byte-identical (same importer clone, same kind mapping, same
`try_range(...).ok()` skip) so `is_authored_request` validation and resolver
outcome keys are unchanged; request enumeration order must not change.

#### [ ] READ-003 — Request "does this id exist" is re-derived by four fetch-and-discard lookups

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/project/resolver.rs:232-240`, `glass-lint-core/src/analysis/project/identities.rs:194-198`, `glass-lint-core/src/analysis/project/linker/export.rs:275-282, 313-319`

Four call sites validate a `ModuleRequestId` by fetching the request through
`interface().request(id)` and discarding it
(`let Some(_request) = ... else { ... }`): `walk_star_exports`
(resolver.rs:232-240, returns `saw_unknown`), `collect_exported_identities`
(identities.rs:194-198, `continue`), `resolve_namespace_export` and
`resolve_request_export` (export.rs:275-282 and 313-319, return
`ExportResolution::Unknown`). Each caller rebuilds the same
module → interface → request-index existence check and then independently
decides the failure semantics, so the existence invariant of the id domain is
not owned by `ModuleInterface` and the failure handling can drift.

**Recommendation:** Add one narrow existence operation on `ModuleInterface`
(e.g. `has_request(ModuleRequestId) -> bool` built on the existing private
`index()`), replace the four discarded fetches with it, and keep the
per-caller failure semantics (`saw_unknown` vs `Unknown` vs `continue`)
unchanged. Guardrails: do not fuse the different fallback behaviors into a
shared helper — only the existence check deduplicates; the two
`export.rs` callers must still return `ExportResolution::Unknown` when absent,
and `walk_star_exports` must still record `saw_unknown`.

### Value arena and static values

#### [ ] READ-002 — Consumers rebuild `StaticObject`/`RootedMember` extraction that `ValueTable` should own

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/model/value.rs:280-285`, `glass-lint-core/src/analysis/flow/matcher.rs:108-126`, `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:230-238`

`ValueTable` owns value-arena accessors `static_string(id)` (value.rs:280-285)
and `binding_slot(id)` (value.rs:268-273), but the analogous `StaticObject`
and rooted-chain views are rebuilt by consumers from a raw
`match values.resolve(id) { Value::StaticObject(o) => Some(o), .. }` / 
`Value::RootedMember { path } => Some(path)` split. The two known instances
are `ArgumentData for CallArgInfo` in flow/matcher.rs:113-126 and
`argument_with_overlay` in matching/arguments/evaluator.rs:230-238. Both do
the same resolve-then-variant-match over the same two variants, so the
"what value does this id carry" interpretation is duplicated rather than owned
by the arena.

**Recommendation:** Add `ValueTable::static_object(id) -> Option<&StaticObject>`
and `ValueTable::rooted_member(id) -> Option<&NamePath>` beside `static_string`,
each delegating to the existing `resolve` (so `Value::Binding` chains keep
resolving), and rewrite the two call sites to call them. Delete the duplicated
match arms. Guardrails: the accessors must keep resolving through binding
chains exactly like `static_string` does, and `evaluator.rs` must preserve the
fall-through `(None, None)` for unknown/non-static values so matcher behavior
and evidence order are unchanged.

#### [ ] READ-005 — Interning surface pushes `intern_value_with_binding(.., None)` onto 9 of 11 production calls

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/value.rs:156-208`, `glass-lint-core/src/analysis/resolution/expression/static_values.rs:102-148`

`intern_value` (value.rs:157-197) is private and the only production-facing
intern entry is `intern_value_with_binding` (value.rs:199-208), whose `binding:
Option<BindingKey>` parameter wraps the value in `Value::Binding`. Nine of the
eleven production calls pass `None` (static_values.rs:105-146, call.rs:87,
expression.rs:346); only `intern_bounded_const_value` (resolution/constant.rs:95)
and the bound-argument path (resolution/call.rs:114) actually use a binding.
Callers that never bind must still type the two-argument name and the `None`
argument, and the private `intern_value` path is duplicated in it.

**Recommendation:** Expose plain `intern_value` as `pub(in crate::analysis)`
and route the nine no-binding call sites through it, leaving
`intern_value_with_binding` as the explicitly-named wrapper for the two
binding-aware sites (better, rename the wrapper to make the binding intent
explicit). Guardrails: keep the capacity/pop/exhausted and terminal-cache
behavior identical for both entry points, and do not change the `Value::Binding`
chaining used by the two binding-aware callers.

### Scope and binding identity model

#### [ ] READ-004 — Parallel ID newtypes drifted in width, sentinel, and test surface

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:16-29, 51-103`, `glass-lint-core/src/analysis/model/module.rs:114-121`, `glass-lint-core/src/analysis/model/value.rs:9-57`

The retained ID family is not consistently encapsulated: `ScopeId(usize)`
(scope.rs:17) and `ModuleRequestId(usize)` (module.rs:115) are pointer-width
while `BindingId`, `BindingVersion`, `FunctionId`, `ValueId`,
`ResolvedObjectId`, and `FlowObjectId` are `u32` (scope.rs:52, 66, 80;
value.rs:10, 30, 46). `ScopeId` derives `Default` (scope.rs:16), so
`ScopeId::default()` yields `ScopeId(0)` — a live program scope id, not a
sentinel — while only `ValueId` defines an explicit `UNKNOWN` sentinel
(value.rs:13) and the other ids have no sentinel discussion at all. Each id
also re-implements near-identical `#[cfg(test)] from_test` (and, for `ScopeId`,
`index_for_test`) constructors, so there is no single convention to copy when a
new id is added.

**Recommendation:** Unify the family on one convention documented once at the
top of `model/scope.rs` and `model/value.rs`: pick `u32` for all arena indices,
remove the `Default` derive from `ScopeId` (no production caller uses it and it
fabricates a valid scope), and consolidate the repeated
`from_test`/`index_for_test` test constructors behind one shared test helper or
macro. Guardrails: no production code path may construct a `ScopeId`/id out of
thin air after the change — all ids must continue to originate from their
allocating collections (`LexicalScopes`, `BindingIndex`, `ValueTable`,
`ModuleInterface`); `ValueId::UNKNOWN` must remain index 0.

## Systemic Themes

- **Adapter methods keep phase boundaries honest.** `ModuleInterface` request
  accessors, `ScopeGraph`/`FrozenScopeGraph` delegation, and the
  `const_value_to_provenance` / `provenance_to_const_value` pair are narrow,
  well-documented bridges; the exceptions are READ-001 (project types pulled
  into the model) and READ-002 (arena interpretation rebuilt at consumer sites).
- **Existence-checks are re-derived instead of owned.** Passim, consumers
  answer "is this id live" by re-fetching and discarding the payload (READ-003,
  and the `resolve_namespace`/`resolve_request` guards), signalling that some
  id-domain invariants belong on `ModuleInterface` and `ValueTable`.
- **Newtype conventions drift within one family.** Width, sentinel, `Default`,
  and test-constructor spelling differ across the id types that caller code
  already treats interchangeably as opaque keys (READ-004).
- **The scope and value static-shape models are deliberately parallel but
  undocumented as such.** `BindingProvenance::StaticString/StaticNumber/
  StaticStringArray/StaticObjectKeys/StaticObjectValues` (scope/provenance.rs:
  45-50) and `Value::StaticString/StaticNumber/StaticArray/StaticObject`
  (value.rs:126-130) are separate phase-local vocabularies, each with its own
  `ConstValue` adapter; this is defensible given build order but worth one
  comment naming the lifecycle split.

## Open Questions

1. Should `BindingProvenance`'s static variants and `Value`'s static variants
   ever share a shape token? Scope provenance is frozen before the value arena
   exists, so unifying them would reparent construction order; left open rather
   than recommended.
2. Is there a plan to align the ptr-width ids (`ScopeId`, `ModuleRequestId`)
   with the `u32` ids, or is the width split deliberate for cache friendliness?
3. Should `StaticProperties::to_const_object` become the single `ConstValue`
   projection for both static-shape models, or stay scope-phase-specific?

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