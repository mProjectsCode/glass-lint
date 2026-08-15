# Codebase Readability Audit — glass-lint-core Chunk 12: Retained module, scope, and value models

## Summary

Chunk 12 covers the retained semantic model of `glass-lint-core`: module
interfaces (`analysis/model/module.rs`), lexical scope and binding identity
(`analysis/model/scope.rs` + `scope/provenance.rs`), bounded static property
collections (`analysis/model/static_properties.rs`), and the resolved value
arena (`analysis/model/value.rs`). The chunk is generally well encapsulated:
state lives behind private fields, identity is expressed through semantic
newtypes, and uncertainty (`Unknown`, `exhausted`, `joined`, contradiction)
is kept distinct from successful-empty results and preserved fail-closed.

The highest-value problems are structural duplication inside the retained
model: an immediately-consumed `ExportObservation` bundle that duplicates the
stored `ExportEntry` and produces a classification enum whose result is always
discarded; a `ValueConstruction` enum that re-declares most of `Value` and is
fanned out through an exhaustive 1:1 match; and a `ModuleInterfaceBuilder`
facade that forwards a surface the underlying `ModuleInterface` already
exposes with `pub` visibility, so its narrower visibility enforces nothing.
Smaller issues cover stranded/duplicated identity conversions, a
storage-only wrapper, a one-field forwarding wrapper, and a stored field that
is a total function of another field.

## Findings

### Retained module model

#### [x] READ-001 — Parallel `ExportObservation`/`ExportEntry` records, a field-copy constructor, and a discarded merge classification

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/model/module.rs:58-154,305-317`

`ExportObservation` (`module.rs:65-70`) has the identical three fields as the
stored `ExportEntry` (`module.rs:58-63`). The three `add_*` export methods
(`module.rs:293-303`) build an `ExportObservation` that is consumed
immediately by `observe_export` (`module.rs:305-317`), where the vacant path
copies it field-for-field through `ExportEntry::from_observation`
(`module.rs:101-108`). `merge` (`module.rs:116-149`) returns an
`ExportMerge` classification (`module.rs:95-99`) whose value is always
discarded at `module.rs:314` (`let _ = entry.get_mut().merge(observation)`).
Adding any future export channel (e.g. a resolved static-value kind) must
touch `ExportEntry`, `ExportObservation`, `from_observation`, `merge`, three
constructors, and the tests, and the two same-shaped structs can silently
drift apart.

**Recommendation:** Delete `ExportObservation` and `ExportMerge`. Have the
three `add_*` methods pass their single channel (`Option<ModuleExport>`,
`Option<FunctionId>`, `Option<String>`) directly to `observe_export`, which
inserts a fresh `ExportEntry` for a vacant name and otherwise calls one
`observe` method on `ExportEntry` that runs the merge logic uniformly for
both paths and returns `()` (no caller consumes the classification).
Guardrail: preserve the exact contradiction semantics — any conflicting
channel observation `mark_unknown`s the entry (resolution
`Some(ModuleExport::Unknown)`, other channels cleared), the three channels
merge independently, and an unknown entry stays unknown for subsequent
observations.

**Fix Applied:** Deleted `ExportObservation` and `ExportMerge`; the three `add_*` methods pass their single channel to `observe_export`, which inserts a fresh `ExportEntry` for vacant names and otherwise calls `ExportEntry::observe` with the same merge semantics and no classification return.

#### [x] READ-002 — `ModuleInterfaceBuilder` facade forwards a `ModuleInterface` surface that is already `pub`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/interface/mod.rs:15-107`; `glass-lint-core/src/analysis/model/module.rs:214-424`

`ModuleInterfaceBuilder` (`facts/interface/mod.rs:15-107`) re-declares the
`ModuleInterface` surface (`record_local`→`add_local`, `add_import_request`,
`add_reexport_request`, `add_star_export_request`, `mark_unknown_exports`,
`has_exports`, and the export adders) as one-line forwards. Every forwarded
method is already `pub` on `ModuleInterface` (`model/module.rs:215-350`),
which is `pub struct` (`model/module.rs:157`) inside `pub mod model`
(`analysis/mod.rs:17`). The builder's narrower `pub(in crate::analysis::facts)`
visibility therefore enforces nothing: any crate module can reach the same
mutators through the model type directly. The builder's non-forward content
(`record_local_imports`, `record_module_request`, `record_pattern_locals`)
and the facts-side record methods (`exports.rs`, `commonjs.rs`) are plain
inherent impls that do not need a facade type.

**Recommendation:** Collapse the facade into the owner: delete
`ModuleInterfaceBuilder`, make `ModuleInterface`'s construction methods
`pub(in crate::analysis)` (or `pub(crate)`), and keep `record_*` methods as
inherent impls on `ModuleInterface` inside `facts/interface`. Guardrail:
model stays free of AST/SWC parsing and request-recognition logic — the
`record_*` methods remain in the `facts` module; only the pass-through layer
is deleted, and `ModuleInterface`'s read surface (iterators, lookups) stays
unchanged for `semantic`, `project`, and `flow` consumers.

**Fix Applied:** Deleted `ModuleInterfaceBuilder`; `FactBuilder` now owns a `ModuleInterface` directly, the `record_*` methods (pattern locals, local imports, module request) and the exports/commonjs record impls live on `ModuleInterface` inside `facts/interface`, and the construction mutators were narrowed to `pub(in crate::analysis)`.

#### [x] READ-007 — `ModuleRequest.kind` is a total function of `ModuleRequest.role`, stored and guarded by a test

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/model/module.rs:34-40,196-212,219-291,319-341`; `glass-lint-core/src/analysis/model/module/tests.rs:75-99`

`ModuleRequest` stores both `kind: ResolutionRequestKind` and
`role: ModuleRequestRole` (`module.rs:36-37`). Every constructor pairs them
(`module.rs:240-291,319-341`): `Import`, `ReExport`, and `StarExport` always
map to
`StaticImport`, `DynamicImport` to `DynamicImport`, `Require` to `Require`.
The invariant is only documented implicitly by the test
`request_constructors_retain_their_valid_kind_and_role_pair`
(`tests.rs:75-99`). A future role can silently violate the pairing, and the
redundant field must be kept consistent on every construction.

**Recommendation:** Store only `role` and derive `kind()` from it (a match on
`ModuleRequestRole`), deleting the `kind` field and the per-constructor
argument. Guardrail: `ResolutionRequestKey::new(importer, request.kind(),
range)` (`module.rs:419`) must keep producing the identical kind values so
resolution-record identity and project budgets are unchanged.

**Fix Applied:** Deleted the stored `kind` field from `ModuleRequest` and the per-constructor argument; `kind()` now derives the kind from `ModuleRequestRole` with the same mapping, and the constructor test now guards the derived mapping.

### Value arena

#### [x] READ-003 — `ValueConstruction` re-declares `Value` and is fanned out through an exhaustive 1:1 match

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/model/value.rs:119-171,242-279`; callers `glass-lint-core/src/analysis/resolution/expression/static_values.rs:120-166`, `glass-lint-core/src/analysis/resolution/constant.rs:76-101`, `glass-lint-core/src/analysis/resolution/call.rs:95,107-125`, `glass-lint-core/src/analysis/resolution/expression.rs:348`

`ValueConstruction` (`value.rs:152-171`) mirrors `Value` (`value.rs:119-133`)
for 10 of its 12 variants (all but `Binding`; the single `StaticObject`
variant is split into `StaticObjectShape` and the name-interning
`StaticObject`), and `intern_construction` (`value.rs:242-279`)
destructures every variant into the identical `Value` — a repeated
destructure/rebuild transform that must be kept in sync whenever `Value`
grows a variant. Only `ValueConstruction::StaticObject { values, names }`
actually needs pre-interning state (string keys plus a `&NameTable`
borrow); the remaining variants are already the final `Value`. The helper
methods in `static_values.rs` (`static_string`, `static_number`,
`static_array`, `static_object_shape`, `intern_object_id`, `rooted_member`),
the callable and provenance interns in `call.rs:95,107-125`, and the global
intern in `expression.rs:348` each wrap a single
`intern_construction(ValueConstruction::…)` call, so the construction enum is
an intermediate layer between helpers that already know the target variant and
`intern_value`/`intern_value_with_binding` (`value.rs:193-240`).

**Recommendation:** Delete `ValueConstruction` and `intern_construction`; make
`intern_value_with_binding` `pub(in crate::analysis)` and call it directly
with `Value` from the `static_values.rs`, `constant.rs`, `call.rs`, and
`expression.rs` helpers. Keep the name-table object path as a dedicated
`intern_static_object`-style method on `ValueTable` (a `pub(in crate::analysis)`
version of the test-only method at `value.rs:296-308`) so the `&NameTable`
lifetime stays inside `ValueTable`.
Guardrail: preserve fail-closed behavior — unresolved name or over-budget
shape still marks the table `exhausted` and returns `ValueId::UNKNOWN`
(`value.rs:263-275`), and binding wrapping in `intern_value_with_binding`
is unchanged.

**Fix Applied:** Deleted `ValueConstruction` and `intern_construction`; `intern_value_with_binding` is now `pub(in crate::analysis)` and the `static_values`, `constant`, `call`, and `expression` helpers build `Value` directly. The `&NameTable` object path became a `pub(in crate::analysis)` `intern_static_object` on `ValueTable` (with an optional binding), preserving fail-closed `UNKNOWN` on unresolved names and over-budget shapes.

#### [x] READ-004 — `FunctionId` conversion surface duplicated and the `IdIndex` impl stranded in `value.rs`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:49-99`; `glass-lint-core/src/analysis/model/value.rs:358-362`; caller `glass-lint-core/src/analysis/facts/stream.rs:300`

`FunctionId` exposes two equivalent conversions — `raw()` (`scope.rs:85-87`)
and `impl From<FunctionId> for u32` (`scope.rs:95-99`) — while its
`IdIndex` impl lives in `value.rs:358-362`, far from the type and its `From`
impl in `scope.rs`, even though `IdIndex: Copy + Into<u32>` is satisfied only
by that `From`. `raw()` has exactly one caller (`facts/stream.rs:300`), which
could use `u32::from(id)`. Across the ID newtype family the surface is
inconsistent: `ValueId` has `raw()` (`value.rs:19-21`), other IDs expose
nothing, and backing widths differ (`ScopeId(usize)` `scope.rs:15`,
`ModuleRequestId(usize)` `module.rs:167` vs `u32` for the rest).

**Recommendation:** Move the `IdIndex` impl next to `FunctionId` in
`model/scope.rs` (reusing `FunctionId::new`), and delete `raw()` in favor of
`u32::from`, updating the single caller. Guardrail: `From<FunctionId> for u32`
is a public contract used by `IdIndex` and must remain; do not unify the
backing widths of `ScopeId`/`ModuleRequestId` with the `u32` IDs in the same
change — that is a separate storage decision.

**Fix Applied:** Moved the `IdIndex` impl next to `FunctionId` in `model/scope.rs` next to its `From<FunctionId> for u32` impl, deleted `raw()`, and switched the `facts/stream.rs` callers to `u32::from(id)`.

### Scope model

#### [x] READ-005 — `ScopeBindings` is a one-field storage newtype with no domain operations

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/scope.rs:240-241,279-319`

`ScopeBindings(HashMap<NameId, BindingProvenance>)` (`scope.rs:240-241`) has
no methods of its own; every access goes through `self.bindings.0` in the
seven `LexicalScope` methods (`insert_binding`, `update_binding`, `binding`,
`has_binding`, `binding_names`, and the two test-only accessors `has_bindings`
and `binding_entries`, `scope.rs:279-319`). The wrapper adds no vocabulary,
invariant, or operation — `LexicalScope` is the real owner of the binding map.

**Recommendation:** Inline the field as
`bindings: HashMap<NameId, BindingProvenance>` on `LexicalScope`, keeping it
private and updating the field declaration, the `new` constructor, and the
seven access methods. Guardrail: the map must remain private to
`LexicalScope`; no caller currently reaches it directly and none should after
the change.

**Fix Applied:** Inlined `ScopeBindings` as a private `HashMap<NameId, BindingProvenance>` field on `LexicalScope`, updating the constructor and all seven access methods; the map stays private.

#### [ ] READ-006 — `CallableValue` is a one-field wrapper whose only member access is a passthrough getter

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/model/value.rs:135-148`; consumer `glass-lint-core/src/analysis/resolution/call.rs:156`

`CallableValue { target: ValueId }` (`value.rs:136-138`) is built only at
`value.rs:256` and read only at `call.rs:156` via `CallableValue::target()`.
The "callable" vocabulary is already carried by the enclosing variant
`Value::Callable` (`value.rs:130`), so the struct adds no invariant or
meaning beyond a forwarding getter.

**Recommendation:** Replace `Value::Callable(CallableValue)` with
`Value::Callable(ValueId)` and drop `CallableValue`, updating the two sites
and the constructor/accessor tests (`model/value/tests.rs:43-47`). Guardrail:
the value must remain hashable/equatable and be resolved identically by
`resolve`/`static_string`; no other caller reads `CallableValue`.

**Fix Applied:** None so far.

## Systemic Themes

- **Construct-vs-retained duplication:** two places in the chunk keep a
  "construction/observation" shape in parallel with the retained shape
  (`ExportObservation`/`ExportEntry`, `ValueConstruction`/`Value`), each with
  a hand-written conversion. Both have a single lifetime-bound reason to
  exist and can be narrowed to a dedicated path instead of a mirrored enum or
  struct (READ-001, READ-003).
- **Boundary enforced by visibility only, not by ownership:** the model's
  mutators are `pub` while a same-crate facade claims a narrower surface
  (READ-002); trait impls and conversions for a type sit in a different model
  module (READ-004). The chunk's read APIs are consistently narrow and
  storage-free; it is the write/construction surface that is inconsistent.
- **Identity newtypes are disciplined:** `ScopeId`, `BindingId`,
  `BindingVersion`, `FunctionId`, `ValueId`, `ResolvedObjectId`,
  `FlowObjectId`, and `ModuleRequestId` are all opaque, ordered/hashable, and
  constructed only through owning allocators; `BindingKey`/`BindingSlot`
  correctly separate versioned identity from version-stable slots. No change
  proposed to this family's design.
- **Uncertainty states are preserved:** `ProvenanceAlternatives` keeps
  unknown/exhausted/joined distinct from retained witnesses, per-entry
  `ModuleExport::Unknown` is distinct from the whole-interface
  `unknown_exports` flag, and arena exhaustion maps to `ValueId::UNKNOWN`
  with fail-closed semantics. None of the findings above collapses these
  states.

## Open Questions

- Resolved: `Value` and `BindingProvenance` share overlapping variants
  (`Local`, `StaticString`, `StaticNumber`, `ModuleExport { module, export }`)
  but live at different lifecycle phases and convert through disjoint type
  pairs: `ConstValue ↔ BindingProvenance`
  (`scope/static_value.rs:16-41,44-62`) and `SymbolCallProvenance ↔ Value`
  (`resolution/call.rs:101-128,135-184`). No third conversion site exists
  today, and no single conversion could serve both pairs, so no consolidation
  is warranted.
- Resolved: `ModuleInterface::has_exports()` (`module.rs:349-351`) indeed
  returns `true` for an entry whose only content is resolution
  `Some(ModuleExport::Unknown)`, so a later `module.exports = {…}` assignment
  wipes the interface via `mark_unknown_exports` (`commonjs.rs:63-66`). This
  is intentional and fail-closed: `mark_unknown_exports` clears every entry
  (`module.rs:343-347`). No unit test composes the two behaviors (module
  tests cover each in isolation, and `commonjs.rs` has no unit tests), so a
  focused test is worth adding if this path is touched.
- Resolved: `ValueTable`'s `terminal_cache` (`value.rs:178`) is index-aligned
  with `values`: `intern_value` pushes exactly one cache entry per newly
  inserted value and none on dedup or early exhaustion (`value.rs:209-232`),
  so `terminal_cache[i]` is the terminal of `values[i]`, with
  `ValueId::UNKNOWN == 0` seeding index 0 in `Default` (`value.rs:181-190`).
  `resolve_terminal` relies on that alignment (`value.rs:341-344`). It is a
  performance cache and should stay; it cannot be derived from `values` at
  construction without re-walking every `Binding` chain, which is the exact
  work the cache avoids.

## Coverage

Audited `glass-lint-core/src/analysis/model/`: `module.rs`,
`scope.rs`, `scope/provenance.rs`, `static_properties.rs`, `value.rs`, and
their unit-test modules. Traced representative callers: `facts/interface/`
(`mod.rs`, `exports.rs`, `commonjs.rs`), `facts/mod.rs`, `facts/stream.rs`,
`facts/arguments.rs`, `scope/` (`mod.rs`, `binding_index.rs`,
`frozen_assignments.rs`, `scope_index.rs`, `graph.rs`,
`query/provenance/callable.rs`, `query/provenance/object.rs`,
`static_value.rs`, `build/provenance.rs`, `build/plan.rs`),
`resolution/` (`constant.rs`, `expression.rs`,
`expression/static_values.rs`, `call.rs`), `project/` (`linker/export.rs`,
`resolver.rs`, `identities.rs`, `state.rs`, `model.rs`), `semantic/mod.rs`,
and `flow/projector/` (`mod.rs`, `history.rs`,
`state/tables/aliases.rs`). Confirmed no `unwrap`/`expect`/`panic` or
`dead_code` allowances inside the chunk files. Verified with `git status
--short` that only this audit file is new.
