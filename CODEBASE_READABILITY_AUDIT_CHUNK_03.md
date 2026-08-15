# Codebase Readability Audit — glass-lint-core Chunk 3: Scope collection frontend

## Summary

Chunk 3 owns the scope-collection frontend: the two-phase
planner-then-collector orchestration in `analysis::scope::mod`,
the binding/function ID index in `scope::binding_index`, the collector
state and artifact bundles in `scope::build`, and the declaration and
assignment analysis helpers in `scope::build::aliases` and
`scope::build::analysis` (`assignment`, `classification`). Its contract
with sibling chunks is the frozen graph boundary: collection produces a
`ScopedProgram` (via `ScopeCollector::freeze` in chunk 4's `freeze.rs`),
`BindingIndex` stores the ID/assignment/function/alias maps consumed by
chunk 5's `ScopeGraph`/`FrozenScopeGraph`, and the analysis helpers feed
the chunk 4 `visitor`.

The code is carefully fail-closed: every collection issue is recorded as
a `ScopeCollectionIssue`, budget exhaustion and name interning mark
`name_exhausted`, and invalid bindings degrade to an issue plus an empty
index rather than a panic. No production `unwrap`/`expect`/`panic` was
found in the chunk's files; discarded errors and stored sentinels are
noted below.

The highest-value issues are: (1) a redundant ScopeId→FunctionId re-key
of parameter aliases whose only consumers convert back to the scope key;
(2) a freeze transition that builds two levels of one-shot bundle
structs, each immediately destructured at a single consumer; (3) a span
predicate duplicated with `LexicalScope::contains` and exported into the
query namespace from a pattern-projection module.

## Findings

### Scope binding index

#### [ ] READ-001 — Parameter aliases are re-keyed ScopeId→FunctionId and converted back on every lookup

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/scope/binding_index.rs:13-23,100-121,188-195`

The collector already produces parameter aliases keyed by `ScopedName`
in the function's own scope (`build/callbacks.rs:24-67`,
`ScopedName::new(function.scope, name)`). At freeze,
`resolve_parameter_aliases` (binding_index.rs:100-111) re-keys these
into a parallel `ParameterAliasKey(FunctionId, NameId)`
(binding_index.rs:13-23), and every consumer immediately converts the
query scope back through `function_for_scope` to re-derive the
`FunctionId` (`graph/storage.rs:34-41`; called from `graph.rs:165,371`,
`query/provenance/callable.rs:187`, `query/bindings.rs:70`). No caller
ever queries parameter aliases by `FunctionId` directly, so the parallel
key type, its resolve step, and its error path are pure indirection that
must be kept in sync with the scope↔function bijection. Relatedly,
`BindingIndexError` (binding_index.rs:43-46) carries a `scope` payload,
but the sole consumer discards it (`build/freeze.rs:38` `unwrap_or_else(|_| ...)`),
so that field is dead data today.

**Recommendation:** Keep `parameter_aliases` keyed by `ScopedName` and
make the lookup a single direct map access (e.g.,
`parameter_alias_for_scope(scope, name)`), deleting `ParameterAliasKey`,
`resolve_parameter_aliases`, and the scope→function hop in
`storage.rs`. Guardrails: leave `function_ids`/`function_spans`, the
`FunctionId`-returning query API (`enclosing_function_at`,
`function_containing`, `BindingKey`), and the fail-closed
`InvalidBindingIndex` issue path untouched; `resolve_function_targets`
for `function_bindings`/`function_aliases` may remain since those
values are consumed as `FunctionId`, but its error payload should be
narrowed to the field actually read.

**Fix Applied:** None so far.

#### [ ] READ-002 — Freeze transition builds two levels of one-shot bundle structs that are immediately destructured

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/build/mod.rs:52-115`, `binding_index.rs:25-41`, `build/freeze.rs:18-41`, `graph.rs:176-227`

The freeze transition assembles four bundle structs, three of which are
constructed and immediately destructured at a single consumer with no
invariant or added vocabulary: `BindingAllocation`
(`freeze.rs:30` → destructured `binding_index.rs:64-68`),
`BindingFreezeInput` (`freeze.rs:31-37` → destructured
`binding_index.rs:61-72`), and the nested pair
`FrozenScopeCollectionArtifacts` → `FrozenPropertyArtifacts` (sealed at
`build/mod.rs:92-102`, peeled twice: `freeze.rs:18-22` then
`graph.rs:180-184`). Because `FrozenScopeCollectionArtifacts` nests the
inner bundle, `freeze.rs` cannot destructure the three property vectors
directly. The pairing is also inconsistently encapsulated:
`FrozenScopeCollectionArtifacts` is `pub(super)` with private fields
while its inner `FrozenPropertyArtifacts` is `pub(in crate::analysis)`
with `pub(in crate::analysis)` fields (build/mod.rs:105-115).

**Recommendation:** Flatten the property facts directly onto
`FrozenScopeCollectionArtifacts` and pass the three vectors to
`finish_collected_properties` by value; fold the three `BindingAllocation`
maps flat into the freeze input (or have `allocate_ids` return the maps)
so the transition has at most one bundle level. Guardrails: each
collection must be consumed exactly once across the transition, the
`InvalidBindingIndex` fallback must stay, and if the build→graph module
boundary genuinely needs a named bundle, keep one (`FrozenPropertyArtifacts`)
rather than two nested layers.

**Fix Applied:** None so far.

### Pattern projection and alias collection

#### [ ] READ-003 — Span-containment predicate is duplicated and exported from the wrong module

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/aliases.rs:140-142`, `analysis/model/scope.rs:275-277`, `analysis/scope/query/mod.rs:14`, `query/provenance/chain.rs:7,125,282,314`

`contains(outer: Span, inner: Span)` lives in `build::aliases.rs`, a
pattern-projection module, and is re-exported into the query namespace
(`query/mod.rs:14`) so `query/provenance/chain.rs` can use it. It is
byte-identical to `LexicalScope::contains`
(`analysis/model/scope.rs:275-277`), so span containment is implemented
twice, and the query layer reaches across the collection-phase module
boundary to borrow a projection helper.

**Recommendation:** Implement containment once at a neutral owner (an
inherent method on the span type in a syntax utility, or route callers
through `LexicalScope::contains`), and delete the aliases.rs free
function plus the `query/mod.rs` re-export. Guardrails: the predicate is
a plain inclusive containment (`outer.lo <= inner.lo && outer.hi >= inner.hi`)
used to test an assignment's scope span against a use span — semantics
must not change in `chain.rs`.

**Fix Applied:** None so far.

#### [ ] READ-004 — `collect_value_aliases` and `collect_assignment_aliases` duplicate the projection/error sequence

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/aliases.rs:26-76,79-137`

The two alias-collection methods are the same "build append closure →
`project_destructuring` → match Unsupported/Exhausted" sequence
differing only in the `is_assignment` flag, the span, and the Ok-arm
sink (`update_binding` vs `record_assignment`). Within the same file,
`collect_require_aliases` mixes conversion vocabulary: the
`ObjectPatProp::Assign` arm rebuilds a `SmolStr` with
`module.as_str().into()` (aliases.rs:107) while the sibling helper
`collect_require_export_alias` takes `&str` and calls `module.into()`
(aliases.rs:132).

**Recommendation:** Collapse the two methods into one helper
parameterized by the assignment flag and a small sink (or an enum), so
the projection and error handling exist once. Guardrails: declaration
aliases must keep updating binding provenance while assignment aliases
append an `AliasAssignment`; the `Exhausted` arm must keep setting
`name_exhausted` and the `Unsupported` arm must remain a no-op.

**Fix Applied:** None so far.

### Collector interning surface

#### [ ] READ-005 — `ScopeCollector::interned_name` is a byte-identical duplicate of `ScopeCollector::name_id`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/scope/build/collector.rs:121-123,132-134`

`name_id` and `interned_name` have identical bodies
(`self.lexical.names.lookup(name)`), identical `&self` signatures, and
identical `pub(super)` visibility; `interned_name` is used at exactly
one site (`build/visitor.rs:336`) while `name_id` is used in several
(`visitor.rs:304`, `assignments.rs:168,309`, `collector.rs:92`). Two
spellings for one lookup invite divergent semantics later.

**Recommendation:** Delete `interned_name` and update `visitor.rs:336`
to call `name_id`. Guardrails: the surviving method must keep returning
`None` (never interning) for names absent from the table and must not
charge the budget — interning plus budget charging stays in
`lookup_or_intern_name`.

**Fix Applied:** None so far.

### Collector state structs

#### [ ] READ-006 — `unknown_provenance` stored sentinel and hand-rolled `Default` in the collection state

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/scope/build/mod.rs:163-177,211-236`, `build/assignments.rs:178`

`PathCollectionState.unknown_provenance` (build/mod.rs:216) is
initialized once to `BindingProvenance::Local` in `Default` and never
mutated; it exists only to lend a stable `&BindingProvenance` fallback
at `assignments.rs:178`. Because `BindingProvenance::Local` is a unit
variant it can be a `const`, so the field is mutable-looking state that
is actually a constant. Separately, `AssignmentCollectionState`
hand-rolls a `Default` (build/mod.rs:169-177) that only assigns
`Vec::new()`/`HashMap::new()`, while the sibling `FunctionCollectionState`
derives `Default` (build/mod.rs:153-160) — inconsistent.

**Recommendation:** Replace the sentinel field with a `const` (e.g.,
`const UNKNOWN_PROVENANCE: BindingProvenance = BindingProvenance::Local;`)
and return `&UNKNOWN_PROVENANCE`; derive `Default` for
`AssignmentCollectionState`. Guardrails: the fallback must remain
`BindingProvenance::Local` (collection-time unknown is treated as a
fresh local root, which is fail-closed), and `PathCollectionState::default()`
must keep its `DEFAULT_ALTERNATIVE_LIMIT`, `reachable = true`, and
`BindingProvenance::Local` fallback semantics.

**Fix Applied:** None so far.

### Declaration classification

#### [ ] READ-007 — `DeclarationClassification::Binding` uses `String` while sibling variants use `SmolStr`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/scope/build/analysis/classification.rs:10-22,225-235`

`DeclarationClassification::Binding` carries `name: String` while
`Require` carries `module: SmolStr` (classification.rs:12-17). `binding()`
allocates via `name.to_owned()` on every binding classification, even
though the consuming sink `ScopeCollector::update_binding` accepts
`impl Into<SmolStr>` (`collector.rs:84-99`) and the same name is already
interned. Two value types for the same string-shaped payload in one enum
force a needless allocation and a conversion at the sink.

**Recommendation:** Carry the binding name as `SmolStr` (as `Require`
does) and drop the `.to_owned()`. Guardrails: `update_binding`'s
`impl Into<SmolStr>` contract is unchanged, and destructuring patterns
must keep producing `None` for the name via `declaration_name`
(pattern-ident-only).

**Fix Applied:** None so far.

## Systemic Themes

- **Budget-charge + intern + exhausted idiom is repeated ~10 times** across
  the planner and collector passes: `build/plan.rs:60-64,93-103,169-193`,
  `build/collector.rs:72-77,101-119`, `build/visitor.rs:174-186,243-246,378-382`,
  `build/assignments.rs:93-99`. Each site is
  `budget.try_charge(); intern/lookup; if None { name_exhausted = true }`.
  A shared `charge_and_intern(&mut self, &str) -> Option<NameId>` on the
  planner and collector would consolidate the flag and the ordering. The
  defining files are chunk 4, but the surface is consumed by this chunk's
  `analysis/aliases` and `analysis/classification` code, so this is a
  cross-chunk consolidation target; budget charging must stay exactly once
  per interning attempt to preserve deterministic operation counts.
- **Parallel provenance precedence chains:** `assignment_provenance`
  (`analysis/assignment.rs:21-33`), `argument_provenance`
  (`build/provenance.rs:119-138`), and `classify_candidates`
  (`analysis/classification.rs:180-223`) each re-walk overlapping probes
  (`module_alias_provenance`, `const_provenance`,
  `returned_object_provenance`, `rooted_name_path`, `static_object_values`)
  with deliberately different ordering. The ordering differences are pinned
  by `analysis/tests.rs` (e.g., bound-callable-over-rooted-alias,
  returned-object-not-constant), so this is treated as policy, not an
  accidental duplicate — see Open Questions.
- **Mixed string-keyed vs NameId-keyed graph surfaces:** the collection
  phase `ScopeGraph` takes `&str` for binding lookups (graph.rs:146-173)
  while `FrozenScopeGraph` takes `NameId` (graph.rs:376-434), and a
  `binding_version_at(scope, &str, span)` wrapper is duplicated in
  graph.rs:271-273 and `query/bindings.rs:131-141`. Chunk 3's
  `BindingIndex` deliberately exposes only ID-keyed lookups, so the
  string↔ID conversion lives in the sibling chunks.

## Open Questions

- Can the three provenance precedence chains (assignment / argument /
  classification candidates) share one policy representation without
  changing the tested orderings in `analysis/scope/build/analysis/tests.rs`?
  A consolidation is only worth it if precedence stays explicit per caller.
- Is the string-keyed collection-phase vs NameId-keyed frozen-phase surface
  split deliberate? If so, a short contract doc on `ScopeGraph`'s binding
  helpers would prevent drift; if not, unifying the key type across the two
  graph phases would remove the duplicated `binding_version_at` wrappers.
- `visible_binding_with_scope` falls back to `BindingProvenance::Local`
  when an assignment's preferred witness is unknown (assignments.rs:178),
  conflating "unknown/incomplete" with "local binding" during collection.
  The frozen graph keeps these distinct via `BindingResolutionStatus`, so
  confirm the collection-time conflation is intended fail-closed behavior
  rather than a lost uncertainty state.

## Coverage

Reviewed (read-only) for Chunk 3 and its direct callers/consumers:

- `analysis/scope/mod.rs` — planner→collector orchestration,
  `ScopeGraph::collect_scoped_program`.
- `analysis/scope/binding_index.rs` — `BindingIndex`, `BindingAllocation`,
  `BindingFreezeInput`, `BindingIndexError`, `ParameterAliasKey`, freeze
  resolve helpers, ID allocation.
- `analysis/scope/build/mod.rs` — `ScopeCollectionArtifacts`/seal,
  `FrozenScopeCollectionArtifacts`, `FrozenPropertyArtifacts`,
  `ScopedDynamicEval`, `FunctionBinding`, `LexicalCollectionState`,
  `FunctionCollectionState`, `AssignmentCollectionState`, `ScopeCollector`,
  `PathCollectionState`, `CollectorCheckpoint`, `FunctionCheckpoint`,
  `ControlFlowFrame`, re-exports.
- `analysis/scope/build/aliases.rs` — value/assignment/require alias
  collection, `contains`.
- `analysis/scope/build/analysis/mod.rs`, `assignment.rs`,
  `classification.rs`, `tests.rs` — provenance/mutability classification,
  declaration classification, `Candidate` precedence.
- Callers/consumers traced: `build/freeze.rs`, `build/collector.rs`,
  `build/visitor.rs`, `build/callbacks.rs`, `build/plan.rs`,
  `build/assignments.rs`, `graph.rs`, `graph/storage.rs`,
  `query/bindings.rs`, `query/functions.rs`, `query/provenance/callable.rs`,
  `query/provenance/chain.rs`, `query/mod.rs`, `analysis/model/scope.rs`,
  `analysis/model/scope/provenance.rs`, `build/tests.rs`.

No production `unwrap`/`expect`/`panic` and no `dead_code` allowances were
found in the chunk's files; the only `expect` calls are in test code.
