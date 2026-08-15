# Codebase Readability Audit — glass-lint-core Chunk 2: Fact interface and stream

## Summary

Chunk 2 owns the fact-building half of `analysis::facts`: the immutable
`FactStream` with its building/frozen phase split (`stream.rs`), the
origin-map checkpoint/rollback machinery (`origin_map.rs`), pattern-leaf
walking (`pattern.rs`), traversal state (`state.rs`), the SWC visitor
(`visitor.rs`), and the module-interface builder (`interface/`). The overall
design is strong: the phase-typed stream makes the freeze ordering
compiler-checked, the issue bitset keeps typed completion outcomes distinct,
the origin map charges the semantic budget before mutation, and `BuiltFacts`
keeps unsupported/budget-exhausted streams distinguishable from valid empty
ones. No wrong-behavior defects were found.

The findings concentrate in three places: (1) the visitor repeats identical
fact-emission blocks across `visit_*` methods; (2) the interface builder has
two parallel pattern-local collection paths and repeats the module-export-name
normalization in two functions; (3) cross-cutting representation choices —
a bare `(SmolStr, SmolStr)` provenance tuple and a parallel
`FrozenFactTables`/`FrozenStorage` pair — force interpretation or conversion
at every boundary. All recommendations are deletions/consolidations onto
existing owners; none propose new abstractions.

## Findings

### Fact stream and freeze boundary

#### [x] READ-001 — `FrozenFactTables` and `FrozenStorage` are parallel two-field wrappers over the same name/value table pair

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:74-83,322-333`, `glass-lint-core/src/analysis/resolution/mod.rs:181-199,239-245`

Both `FrozenStorage { names: NameTable, values: ValueTable }`
(`facts/stream.rs:74-83`) and `FrozenFactTables { names, values }`
(`resolution/mod.rs:181-184`) model the identical two-field bundle, and the
only linkage is a one-way passthrough conversion
`FrozenFactTables::into_storage` (`resolution/mod.rs:191-193`) consumed at a
single site (`stream.rs:327`, inside `freeze`). `FrozenFactTables` adds no
vocabulary or invariant beyond the bundling that `FrozenStorage` already
enforces as the phase storage; keeping two types means any change to the
freeze contract (new table, rename, accessor) must be made twice and a reader
must cross two modules to see the whole transition. The claimed invariant —
that both artifact-local ID spaces cross the freeze boundary together — is
stated in the docs of both types and is equally enforced if only
`FrozenStorage` remains.

**Recommendation:** Delete `FrozenFactTables` and its `into_storage`, and make
`FactStream<Building>::freeze` accept `FrozenStorage` directly
(`FrozenStorage::from_tables` already exists for this). Have
`Resolver::freeze_into` (`resolution/mod.rs:239-245`) build the storage via
`FrozenStorage::from_tables(self.names, self.values)`; keep the `#[cfg(test)]
for_test` constructor on the surviving type and update `facts/tests/stream.rs`
and `facts/mod.rs:222-228` accordingly. Guardrail: keep the names/values pair
crossing atomically in one call so no path can freeze with a half-consistent
table bundle.
**Fix Applied:** Deleted `FrozenFactTables` and its `into_storage`; `freeze`
now takes `FrozenStorage` directly, `Resolver::freeze_into` builds it via
`FrozenStorage::from_tables`, and `FrozenStorage::for_test` replaced
`FrozenFactTables::for_test` in `facts/mod.rs::into_stream` and
`facts/tests/stream.rs`, preserving the single atomic names/values handoff.

### Module interface builder (`interface/`)

#### [x] READ-002 — Parallel pattern-local collection paths: one returns a set that the only caller discards, the other re-collects the same pattern

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/interface/mod.rs:36-53`, `glass-lint-core/src/analysis/facts/interface/exports.rs:43`, `glass-lint-core/src/analysis/facts/mod.rs:241-243`

`ModuleInterfaceBuilder::record_pattern_locals` (`interface/mod.rs:36-45`)
collects the pattern bindings, records them into the interface, and returns
the `BTreeSet<SmolStr>`; the only caller `FactBuilder::record_pattern_locals`
(`facts/mod.rs:241-243`) discards the return value. The same pattern is then
collected a second time from a parallel static helper
`ModuleInterfaceBuilder::collect_pattern_locals` (`interface/mod.rs:47-53`),
used by `record_export_decl`'s `Decl::Var` arm (`exports.rs:43`) to name the
exported locals. Every `export const a = 1, b = 2;` therefore walks its
pattern twice and builds two identical `BTreeSet`s, and the API
(`record_pattern_locals` "records and returns") invites exactly this kind of
misuse.

**Recommendation:** Consolidate onto one collection: since
`record_export_decl` runs before the export's declarators are visited, have
the `Decl::Var` arm (`exports.rs:43`) call the recording variant
`record_pattern_locals` and use its returned names for `add_export`, then
delete the static `collect_pattern_locals`.
`FactBuilder::record_pattern_locals` (`facts/mod.rs:241-243`) keeps its
discarded return as the plain recording wrapper.
Guardrail: `visit_var_declarator` must keep recording locals for every
declarator, and `add_local` must stay idempotent (it inserts into a
`BTreeSet`), so the export arm re-inserting the same names is a no-op;
non-`Pat::Ident` patterns (object/array destructuring) must still export
their local names.
**Fix Applied:** The `Decl::Var` export arm now calls
`ModuleInterfaceBuilder::record_pattern_locals` (which records and returns
the collected names) and uses the returned set for the export work; the
static `collect_pattern_locals` helper was deleted. `add_local` stays
idempotent and `visit_var_declarator` still records every declarator, so the
later re-insertion is a no-op and destructured exports keep their names.

#### [x] READ-003 — Module-export-name `(original, exported)` normalization is repeated in two export functions

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/facts/interface/exports.rs:74-78,110-114`

`record_local_named_exports` and `record_reexports` each recompute the
"exported-or-original" name pair verbatim:

```rust
let original = module_export_name(&named.orig);
let exported = named.exported.as_ref().map_or_else(|| original.clone(), module_export_name);
```

at `exports.rs:74-78` and `exports.rs:110-114`. Both also filter
`ExportSpecifier::Named(named) if named.is_type_only` just before. The
duplicated interpretation of `ModuleExportName` (when the `exported` alias is
absent, the exported name equals the original) drifts if either call site
changes independently.

**Recommendation:** Extract one helper that maps an
`ExportNamedSpecifier` to its `(original, exported)` pair (narrowest valid
owner: a private free function or inherent method in `interface/exports.rs`),
and use it in both functions. Guardrail: keep the type-only filtering and the
`original.clone()` reuse of the original name as-is; do not fold
`record_local_named_exports` and `record_reexports` together — their
`ModuleExport` variants differ.
**Fix Applied:** Extracted the private `original_exported_pair` helper on
`ModuleInterfaceBuilder` in `interface/exports.rs` and used it in both
`record_local_named_exports` and `record_reexports`, preserving the type-only
filtering and the `original.clone()` fallback.

### Fact model and visitor

#### [x] READ-004 — `Return.region` and the region on `Break`/`Continue` control facts are dead sentinel fields always set to region 0

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/visitor.rs:386,393,407-413`, `glass-lint-core/src/analysis/model/fact.rs:405-412`, `glass-lint-core/src/analysis/flow/projector/control.rs:35-37,271-293`, `glass-lint-core/src/analysis/flow/projector/driver.rs:92-97`, `glass-lint-core/src/analysis/flow/effect/mod.rs:145-158,431`, `glass-lint-core/src/analysis/matching/build.rs:154-155`

The visitor hard-codes `ControlRegionId::new(0)` for `Break`, `Continue`, and
`Return` facts (`visitor.rs:386,393,410`), yet every consumer ignores it: the
projector routes `Break | Continue | Return` to `transfer_abrupt(kind)`
(`flow/projector/control.rs:35-37,271-293`), which takes only the kind, and
effects match only on the kind (`flow/effect/mod.rs:148-155,431`). Worse,
region 0 is not a reserved sentinel — the first allocated control region *is*
`ControlRegionId::new(0)` (`facts/control.rs:196`, with `next_control_region`
seeded at the default in `state.rs:31`), so the constant is both dead and
collides with a real region identity. The field invites a future consumer to
believe the region is meaningful and to "fix" the wrong-looking constant.

**Recommendation:** Remove the `region` field from `FactPayload::Return` and
split `Break`/`Continue` out of `FactPayload::Control` into standalone payload
variants with no region (only the regioned kinds stay in `Control`). In
`flow/projector/driver.rs:92-97` route the three abrupt facts directly to
`transfer_abrupt(AbruptExit::…)`, delete the now-dead
`Break | Continue | Return` arm from `transfer_control`
(`flow/projector/control.rs:35-37`), and widen that abrupt transfer to
`pub(super)` so the driver can call it. Effects match the new shapes
(`flow/effect/mod.rs:145-158,431`), and the ignore-arm at
`matching/build.rs:154-155` must list the new variants. Guardrail: keep
branch/loop/switch/try region semantics exactly as they are; `Return` must
still carry `value`, and the deterministic fact order of `visit_return_stmt`
(`stmt.arg` visited before the fact is emitted) must not change.
**Fix Applied:** Removed `region` from `FactPayload::Return`, split
`Break`/`Continue` into standalone region-less `FactPayload` variants (and
dropped them from `ControlKind`), routed the three abrupt facts from the
driver directly to the now-`pub(super)` `transfer_abrupt(AbruptExit::…)`, and
updated the effect `Return` match and the matching ignore-arm. Branch/loop/
switch/try region semantics and `visit_return_stmt` ordering are unchanged.

#### [x] READ-005 — The visitor repeats identical fact-emission blocks across `visit_*` methods

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/visitor.rs:47-59` and `164-176`; `visitor.rs:91-103` and `112-124`

The `MemberRead` emission sequence — `resolve_member`, `member_expression_chain`,
`syntactic_path`, the four-field `FactPayload::MemberRead`, then
`visit_children_with` — is duplicated verbatim between `visit_member_expr`
(`visitor.rs:47-59`) and the `OptChainBase::Member` arm of
`visit_opt_chain_expr` (`visitor.rs:164-176`). Likewise the "write to member
with unknown source" sequence — resolve target, derive receiver from a member
object, emit `FactPayload::Assignment { source: ValueId::UNKNOWN, .. }` — is
duplicated between `visit_update_expr` (`visitor.rs:91-103`) and the
`UnaryOp::Delete` arm of `visit_unary_expr` (`visitor.rs:112-124`). These are
the same semantic roles reached through different syntax, and any change to
the `MemberRead` or member-assignment payload shape must be made in two
places that are easy to drift apart.

**Recommendation:** Extract two builder helpers owned by `FactBuilder`
(e.g., `record_member_read(&mut self, member: &MemberExpr)` and
`emit_member_assignment(&mut self, span, arg: &Expr)`), called from both visit
methods. Guardrail: the helpers must preserve the exact traversal order —
children are visited after the parent fact in both `MemberRead` sites, and
`update.arg`/`unary.arg` are visited before the assignment fact — so the
deterministic evidence order asserted by `tests/build.rs` and
`stream_tests.rs` is unchanged.
**Fix Applied:** The `MemberRead` duplication was already removed by chunk 01
(via `FactBuilder::record_member_read` in `reads.rs`). This change extracts
`FactBuilder::emit_member_assignment` in `assignments.rs` and uses it from
both `visit_update_expr` and the `UnaryOp::Delete` arm of `visit_unary_expr`,
leaving the arg visitation (and thus evidence order) at each call site.

### Class provenance representation

#### [ ] READ-006 — Class/instance provenance is a bare `(SmolStr, SmolStr)` tuple interpreted at every use site

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/facts/provenance.rs:7,41-42`, `state.rs:17`, `functions.rs:67`, `calls/callee.rs:27,140`, `construction.rs:25`, `instance.rs:24-26`, `model/fact.rs:231,274,337`, `matching/build.rs:204`

The `(SmolStr, SmolStr)` pair meaning "class identity = (module, export)" is
threaded as a raw tuple through at least eight types and functions: the
`Origin` type alias exists in `provenance.rs:7` but sibling fields such as
`TargetProvenance.instance_origin`/`class_origin` (`provenance.rs:41-42`)
still spell the raw tuple, `TraversalState.class_stack`
(`state.rs:17`), `current_class` (`functions.rs:67`),
`ResolvedCallee.instance_class` and `instance_class_for_receiver`
(`calls/callee.rs:27,140`), `InstanceCallable::class_identity`
(`instance.rs:24-26`), `CallEvent.instance_class` (`model/fact.rs:231,274,337`),
and the `let Some((module, export)) = ...` destructure at `matching/build.rs:204`.
The pair's ordering is an invariant re-derived by every consumer, the same
two-string tuple shape is reused for unrelated purposes elsewhere, and
`TargetProvenance`'s two `Option<(SmolStr, SmolStr)>` fields are
indistinguishable at the type level.

**Recommendation:** Define one semantic newtype owning the module/export pair
(e.g., a `ClassIdentity { module, export }` in `analysis/model`, so both
`facts` and `matching` can use it), give it private fields plus narrow
accessors/`From` conversions, and replace the raw tuple spellings at every
listed site (`provenance.rs`, `state.rs`, `functions.rs`, `calls/callee.rs`,
`construction.rs`, `instance.rs`, `model/fact.rs`, `matching/build.rs`) in one
change; the newtype supersedes the partial `Origin` alias in `provenance.rs:7`.
Guardrail: keep the instance-origin and class-origin channels
(`provenance.rs:33-36`) as separate maps — the newtype must not collapse
distinct provenance channels — and preserve the pair's current by-value
`Clone` semantics (`SmolStr` is not `Copy`).

## Systemic Themes

- **Tuple-as-identity:** The same `(SmolStr, SmolStr)` shape is used for
  class identity, with an existing `Origin` alias that is only partially
  applied; type-level naming would remove cross-module interpretation.
- **Boundary-carried bundles:** The freeze transition ships the
  name/value tables through a dedicated wrapper that duplicates the phase
  storage's shape; single-owner bundles reduce conversion paths.
- **Visitor emission boilerplate:** The visitor delegates well to the
  `record_*` implementation files, but within `visitor.rs` the same payload
  construction is repeated across syntax variants.
- **Interface accumulation:** `ModuleInterfaceBuilder` is a justified facade
  (it adds CommonJS/export/request interpretation), but its pattern-local
  helpers double up collection work.

## Open Questions

- Resolved: `FactStream::function_parameters` (`stream.rs:175-186`) returns
  `Some(&[])` only for `FunctionId::new(0)`, the documented implicit
  program-level slot (`stream.rs:160-162`) that never passes through
  `emit_function_fact`. Every other unregistered id reads back as `None`, and
  the effect builder marks those effects invalid (`flow/effect/mod.rs:389-392`),
  so a `scope_at` failure in `emit_function_fact` (`functions.rs:81`) cannot be
  mistaken for a registered zero-parameter function.
- Resolved: no deliberate placeholder. No consumer reads the region on
  `Break`/`Continue`/`Return` — abrupt routing is kind-only
  (`route_abrupt`/`record_abrupt_exit`, `flow/projector/state.rs:345-393`) and
  effects match on kind only; no comment reserves region 0 as a sentinel, and
  region 0 is a real identity (the first allocated region). READ-004 stands as
  a removal, not documentation.
- Resolved: `analysis::model` already hosts shared producer/consumer
  contracts — `CallEvent` (`model/fact.rs:220`) is produced by `facts` and
  consumed by `matching` (`matching/build.rs:46,204`) — so a shared
  `ClassIdentity` there follows the existing pattern.

## Coverage

- Reviewed `glass-lint-core/src/analysis/facts/stream.rs`,
  `origin_map.rs`, `pattern.rs`, `state.rs`, `visitor.rs`,
  `interface/{mod,commonjs,exports}.rs`, plus supporting builders
  `facts/{mod,functions,control,construction,provenance,instance}.rs`,
  `calls/{mod,callee}.rs`, `model/fact.rs`, `model/scope.rs`,
  `resolution/mod.rs`, `resolution/expression/static_values.rs`,
  `flow/effect/mod.rs`, `flow/projector/{control,driver}.rs`,
  `matching/build.rs`, and `semantic/mod.rs` as callers.
- Verified representative callers for every finding via `rg`; traced
  `Freeze`/`freeze_into`, `mark_name_exhausted`,
  `is_structurally_valid`/`is_valid`, `function_parameters`,
  `property_write_value`, `observe_module_call`, `emit_control`,
  `record_pattern_locals`, and the `Break|Continue|Return` projector path.
- Read-only audit: no source, test, config, Cargo, or documentation file was
  modified; only this report is new.
