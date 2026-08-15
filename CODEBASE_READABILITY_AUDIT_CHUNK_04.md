# Codebase Readability Audit — glass-lint-core Chunk 4: Scope collection subsystems

## Summary

Chunk 4 owns the source-order scope collection pipeline in
`glass-lint-core/src/analysis/scope/build`: declaration planning (`plan`,
`shape`), structural traversal (`traversal`, `visitor`, `collector`,
`bindings`), assignment and control-flow joining (`assignments`,
`assignments/control_flow`, `history`), pattern normalization and projection
(`compact_pat`, `projection`, `aliases`), provenance/constant inference
(`provenance`, `constants`, `analysis/*`), and the freeze transition
(`freeze`, `program`, `mod.rs`).

The subsystem is well-engineered overall: the planner/collector two-pass
design, the parent-linked assignment history, the shape-table scope identity,
and the fail-closed issue recording all preserve the architecture invariants
(bounded work, deterministic output, unknown states distinct from successful
empty). No lifecycle-collapsing or safety regressions were found.

The main readability cost is an unusually large amount of mechanical
duplication between parallel API surfaces:

- `ScopeCollector` defines most control-flow/scope operations twice — once as
  an inherent method and again as a `ScopePass` trait forwarder with the same
  signature (READ-001).
- Declaration registration and binding-scope selection exist in near-identical
  parallel form on `ScopePlanner` and `ScopeCollector` (READ-002).
- Assignment recording, alias projection, and function/arrow completion each
  duplicate their whole body across two near-identical variants
  (READ-004, READ-005, READ-007).

Secondary issues are smaller: a redundant name-lookup helper surface, a
storage-shaped tuple alias, a needless pattern clone, a hand-written `Debug`
that mirrors `derive`, field visibility wider than the owning type, and
invariant panics on paths that already model failure with `Result`.

## Findings

### Assignment and control-flow state

#### [x] READ-001 — `ScopePass for ScopeCollector` is a forwarding shim duplicating the inherent control-flow methods

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/build/visitor.rs:32-118`

`impl ScopePass for ScopeCollector` (visitor.rs:32-118) defines 21 methods —
`push_scope`, `pop_scope`, `current_scope`, `enter_if`, `enter_else`,
`exit_if`, `enter_loop`, `exit_loop`, `enter_switch`, `enter_switch_case`,
`exit_switch_case`, `exit_switch`, `enter_try`, `enter_catch`, `exit_try`,
`enter_function`, `exit_function`, `mark_unreachable`, `break_exit`,
`continue_exit`, `is_budget_exhausted` — of which 19 are pure forwarding shims
whose signatures are identical to the inherent methods in
`assignments/control_flow.rs:6-291` and `collector.rs:50-54, 176-207`
(each is `fn enter_if(&mut self) { self.enter_if(); }`, and the inherent method
silently shadows the trait one). `pop_scope` is not a pure shim: it is an
adapter that guards on `ScopeEntry::Entered` over a differently-typed inherent
`pop_scope()` (collector.rs:200-207). `is_budget_exhausted` (visitor.rs:47) has
no inherent counterpart at all — it reads `self.budget.exhausted()` directly and
is not duplicated. Any change to control-flow joining must now be written and
maintained in two places for the 19 duplicated methods, and the two sets can
diverge silently. This is the single largest mechanical duplication in the
chunk (~85 lines).

**Recommendation:** Make the inherent definitions the trait implementation —
move the 19 duplicated bodies from `assignments/control_flow.rs` and
`collector.rs` into `impl ScopePass for ScopeCollector` and delete the
forwarding block in visitor.rs. Fold the inherent `pop_scope()` body
(collector.rs:200-207) into the trait method behind the existing
`ScopeEntry::Entered` guard, and leave `is_budget_exhausted` as-is (it is
already direct). Update the direct inherent calls in `tests.rs` /
`tests_extended.rs` (e.g. `collector.push_scope(...)`, `collector.pop_scope()`,
`collector.current_scope()`) to go through the trait or `ScopeTraversal`.
Guardrail: keep `ScopeEntry::Rejected`
semantics, the `ScopeStackUnderflow` / `ShapeMismatch` issue recording, and
the fail-closed `current_scope()` returning `None` after an issue; do not
apply this consolidation to `ScopePlanner`, whose trait methods are genuine
adapters over differently-typed inherent methods (plan.rs:148-167).

**Fix Applied:** Consolidated the inherent control-flow bodies and the scope-stack bodies into the single `impl ScopePass for ScopeCollector` in visitor.rs and deleted the forwarding block. The inherent `pop_scope()` body was folded into the trait method behind the existing `ScopeEntry::Entered` guard; `is_budget_exhausted` stayed direct. Deleted `assignments/control_flow.rs` (and its `mod` declaration) since Rust forbids a second trait impl. Updated direct inherent calls in `tests.rs` / `tests_extended.rs` to go through the trait, and added the `ScopePass` import where `self.current_scope()` is called (collector.rs, callbacks.rs). `ScopeEntry::Rejected` semantics, `ScopeStackUnderflow` / `ShapeMismatch` recording, and fail-closed `current_scope()` returning `None` after an issue are preserved; `ScopePlanner` is untouched.

#### [x] READ-002 — Declaration registration and `var` binding-scope selection are duplicated across planner and collector

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/build/plan.rs:93-124`, `glass-lint-core/src/analysis/scope/build/collector.rs:56-174`

`ScopePlanner::insert` / `insert_local` / `insert_import` / `insert_pat_locals`
(plan.rs:93-117) and `ScopeCollector::register_binding` / `register_local` /
`register_pat_locals` (collector.rs:66-82, 162-168) re-implement the same
sequence — charge the semantic budget, intern (or look up) the name, set
`name_exhausted` on failure, then `LexicalScopes::get_mut(scope)
.insert_binding(...)` — on the same underlying types. `update_binding`
(collector.rs:84-99) is *not* part of that sequence: it does not charge the
budget for the name, resolves with the non-interning `name_id`, and calls the
scope's `update_binding`, so `reset_pat_locals` (collector.rs:170-174) stays
with it. `binding_scope` still differs (plan.rs:119-124 vs collector.rs:56-64)
only in that the collector variant guards on collected issues; the
`VarDeclKind::Var` hoisting rule itself is already centralized in
`var_binding_scope` (bindings.rs:73-84), which both passes call. The
exhausted-flag convention is re-encoded in each function, so a change to
budget/exhaustion handling must touch both passes.

**Recommendation:** Extract one shared declaration-registration helper in
`bindings.rs` — charge, intern-or-lookup, fail-close on exhaustion, then
`insert_binding` — over the owning state (`&mut LexicalScopes`, `&mut
NameTable`, `&mut bool name_exhausted`, `&SemanticBudget`), called by the
planner's `insert` and the collector's `register_binding`. Keep
`intern_provenance_strings`, `update_binding`, and `reset_pat_locals` in the
collector, and leave the `binding_scope` methods as thin branches over the
already-centralized `var_binding_scope`. Guardrail: the collector variant must
keep interning provenance strings (`intern_provenance_strings`) and must
return `None` when `artifacts.has_issues()` so invalid collection fails
closed; the planner variant must not gain that guard.

**Fix Applied:** Added `bindings::register_declaration_binding` owning charge → intern → fail-close-on-exhaustion → `insert_binding` over the owning state (`&mut LexicalScopes`, `&mut NameTable`, `&mut bool name_exhausted`, `&SemanticBudget`). The planner's `insert` and the collector's `register_binding` now both delegate to it; `register_binding` keeps its `intern_provenance_strings` step (taking provenance by reference). `update_binding` / `reset_pat_locals` and the provenance-string interning stayed in the collector; both `binding_scope` methods remain thin branches over the centralized `var_binding_scope`, with the collector's `has_issues()` fail-closed guard preserved and the planner variant unchanged.

#### [x] READ-003 — Three name-lookup methods expose overlapping, inconsistently named surfaces

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/scope/build/collector.rs:121-134`, `glass-lint-core/src/analysis/scope/build/provenance.rs:102-110`

`ScopeCollector::name_id` (collector.rs:121) and `ScopeCollector::interned_name`
(collector.rs:132) have byte-identical bodies (`self.lexical.names.lookup(...)`);
`interned_name` has exactly one caller (visitor.rs:336). The naming suggests a
distinction (lookup vs intern) that does not exist, since interning lives only
in `lookup_or_intern_name` (collector.rs:125). Separately,
`require_module_expr_name` (provenance.rs:108) is a one-line forwarder of
`module_request_name` (provenance.rs:102) that adds no argument or invariant —
callers of the former (classification.rs:151, 196) could call the latter
directly with the policy argument.

**Recommendation:** Delete `interned_name` and route visitor.rs:336 through
`name_id`. Delete the one-line wrapper `require_module_expr_name` (widening
`module_request_name` to `pub(super)`) and call
`module_request_name(expr, ModuleRequestPolicy::alias())` directly at its two
call sites (classification.rs:151, 196), keeping the policy-parameterized
`module_request_name` as the single entry point. Guardrail: keep
`lookup_or_intern_name` (which interns) distinct from `name_id` (which does
not) — that difference is real and load-bearing for budget accounting.

**Fix Applied:** Deleted the one-line `require_module_expr_name` wrapper, widened `module_request_name` to `pub(super)`, and updated the two classification call sites to call `module_request_name(expr, ModuleRequestPolicy::alias())` directly, keeping the policy-parameterized function as the single entry point. The `interned_name` deletion had already been applied by chunk 03 read 005; `lookup_or_intern_name` (interning) and `name_id` (non-interning) remain distinct.

#### [x] READ-005 — Assignment recording duplicates the versioning-and-push tail across two variants

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/assignments.rs:102-149`

`record_assignment_value` (assignments.rs:102-119) and `record_join_assignment`
(assignments.rs:121-149) both call `next_assignment_version`, write the
assignment environment, and push an `AliasAssignment`; only the
environment-write kind differs (`record_known` vs
`record_alternatives`/`record_unknown`) and the constructor
(`AliasAssignment::single` vs `::joined`, model/scope/provenance.rs:226-258).
The join variant also re-implements the `has_complete_witness` decision that
`record_alternatives`/`record_unknown` are already about. Note the two are not
called from one shared site: `record_assignment_value` is the general
single-write path (via `record_assignment`, assignments.rs:99), while
`record_join_assignment` is reached only from the join loop (assignments.rs:280).
The overlap is structural — both end in version-bump + environment write +
push — not a duplicated call site.

**Recommendation:** Extract a private tail such as `push_assignment(span,
scope, name, alternatives: ProvenanceAlternatives)` that owns
`next_assignment_version` + the gated environment write + `assignments.push`,
writing `record_alternatives` when the alternatives have a complete witness and
`record_unknown` otherwise. Have `record_assignment_value` pass
`ProvenanceAlternatives::single(provenance)` (semantically identical to its
current `record_known` write) and `record_join_assignment` pass
`value.alternatives()`. This requires a private `AliasAssignment` constructor
over `ProvenanceAlternatives`, since the existing `single` / `joined` take
`BindingProvenance` / `ProvenanceJoin` respectively and cannot build one
variant from the other's input.
Guardrail: keep the exact environment-write order — the join must write
`record_unknown` (not empty) when no complete witness exists so unknown stays
distinct from successful-empty, and the versions must remain strictly
increasing in source order.

**Fix Applied:** Extracted the private `push_assignment(span, scope, name, alternatives)` tail that owns `next_assignment_version` + the gated environment write (`record_alternatives` when the alternatives have a complete witness, `record_unknown` otherwise) + `assignments.push`, with a new private `AliasAssignment::from_alternatives` constructor. `record_assignment_value` now passes `ProvenanceAlternatives::single(provenance)` and `record_join_assignment` passes `value.alternatives()`. Deleted the now-obsolete `AliasAssignment::single`/`joined` constructors, `ProvenanceJoin::into_alternatives`, and `AssignmentEnvironment::record_known`, updating the model/history tests to the single construction/write path. The join still writes `record_unknown` (not empty) without a complete witness, and versions remain strictly increasing in source order.

### Pattern projection and alias collection

#### [x] READ-004 — Declaration and assignment alias projection are near-identical wrappers

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/aliases.rs:26-76`

`collect_value_aliases` (aliases.rs:26-46) and `collect_assignment_aliases`
(aliases.rs:49-76) share the entire shape: build an `append_name_path` closure,
call `project_destructuring` with an `is_assignment` flag, then handle the
identical `ProjectionError::Unsupported` (silently return) and
`ProjectionError::Exhausted` (set `lexical.name_exhausted`) arms. They differ
only in the per-binding write (`update_binding` + `ValueAlias` vs
`record_assignment` + `ValueAlias`). The `projection.rs` module doc already
treats declaration and assignment projection as one table
(`build/projection.rs:8-17`), so the split lives in the callers, not the
domain.

**Recommendation:** Collapse both into one `collect_destructuring_aliases(pat,
target, span_or_none, scope, is_assignment)` core that owns the append closure
and the `Unsupported`/`Exhausted` handling, parameterizing only the write
operation. Guardrail: preserve the exact behavior difference — declarations
`update_binding` (no assignment history entry, no version bump) while
assignments call `record_assignment` (history + version), and `Unsupported`
must stay a silent no-op while `Exhausted` must set `name_exhausted`.

**Fix Applied:** Already satisfied by chunk 03 read 004 (commit 0b8cc159): `collect_value_aliases` and `collect_assignment_aliases` are thin sinks over one `collect_destructuring_aliases` that owns the append closure and the `Unsupported`/`Exhausted` handling, parameterizing only the write operation. Verified the guardrails hold in the current code: declarations call `update_binding` (no history entry, no version bump), assignments call `record_assignment` (history + version), `Unsupported` stays a silent no-op, and `Exhausted` sets `name_exhausted`. No further work needed.

### Visitor completion hooks

#### [x] READ-007 — `after_function` and `after_arrow` duplicate the pending-name and inline-parameter install sequence

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/build/visitor.rs:189-221`, `glass-lint-core/src/analysis/scope/build/collector.rs:209-219`

`ScopeCollector::after_function` (visitor.rs:189-206) and `after_arrow`
(visitor.rs:208-221) are structurally identical: pop `pending_function_names`
and insert a `FunctionBinding` into `function_scopes`, then pop
`inline_parameters` and `record_assignment` each installed parameter. The only
difference is the parameter-compaction source
(`Self::function_parameters(function)` vs `Self::arrow_parameters(arrow)`)
(collector.rs:209-219). The same near-identical pair exists on the planner side
(`after_function` / `after_arrow`, plan.rs:226-236), which both just loop
`insert_pat_locals`. Future changes to callback-parameter handling must be
made in four places.

**Recommendation:** Extract a shared helper, e.g.
`install_function_binding(&mut self, scope, pending, parameters)` that owns the
`function_scopes` insert, and a second helper that owns the
`inline_parameters` install, used by both `after_*` hooks; parameterize only
the `parameters: Vec<CompactPat>` construction. Guardrail: keep the two
separate `after_*` trait hooks (the traversal dispatches on the AST node kind)
and keep the `after_function`/`after_arrow` planner-side loops intact if they
are not merged, since the planner has no inline parameters.

**Fix Applied:** Extracted `install_function_binding` (owns the `function_scopes` insert) and `install_inline_parameters` (owns the inline-parameter `record_assignment` loop); both `after_*` hooks now differ only in the `Vec<CompactPat>` construction (`function_parameters` vs `arrow_parameters`). The two separate trait hooks stay (the traversal dispatches on the AST node kind), and the planner-side `after_function`/`after_arrow` loops are untouched since the planner has no inline parameters.

### Encapsulation and data-shape issues

#### [x] READ-006 — Joined path assignments are a raw 3-tuple that callers destructure and re-pass field by field

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Newtype
- **Location:** `glass-lint-core/src/analysis/scope/build/assignments.rs:18, 43-64, 279-281`

`type JoinedPathAssignments = Vec<(ScopeId, NameId, ProvenanceJoin)>`
(assignments.rs:18) is a storage-shaped alias. `PathCollectionState::join_paths`
builds it (assignments.rs:43-64) and `ScopeCollector::join_paths` immediately
destructures and re-passes each element as three arguments to
`record_join_assignment` (assignments.rs:279-281) — the skill's
"destructure-and-pass-individual-fields" pattern. The `(scope, name, join)`
triple is a domain value (a joined assignment pending install) whose shape is
not self-documenting and whose invariants (non-rooted `NameId`, bounded join)
live only in the caller.

**Recommendation:** Introduce a small private value type (e.g.
`JoinedAssignment { scope, name, value: ProvenanceJoin }`) owned by the
`assignments` module, have `join_paths` produce `Vec<JoinedAssignment>` and
`record_join_assignment` accept it by value, deleting the triple destructuring
at the call site. Guardrail: keep the join bound (`self.alternative_limit`)
fixed at merge start and keep the fallback-to-incoming/unknown handling in
`join_paths`; do not expose the type outside the `build` subtree.

**Fix Applied:** Replaced the `JoinedPathAssignments` triple alias with a private `JoinedAssignment { scope, name, value }` value type owned by the `assignments` module. `PathCollectionState::join_paths` produces `Vec<JoinedAssignment>` and `ScopeCollector::join_paths` hands each value to `record_join_assignment` by value, deleting the triple destructuring at the call site. The join bound stays fixed at merge start (`self.alternative_limit`) and the fallback-to-incoming/unknown handling remains in `join_paths`; the type is not exposed outside the `build` subtree.

#### [ ] READ-008 — Needless deep pattern clone and an `if`-as-`Option` idiom

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/build/visitor.rs:355-365`, `glass-lint-core/src/analysis/scope/build/collector.rs:50-54`, `glass-lint-core/src/analysis/scope/build/analysis/classification.rs:205-211`

`record_destructuring_assignment` (visitor.rs:355-365) does
`let pattern: Pat = pattern.clone().into()` to hand `collect_assignment_aliases`
a `&Pat`, deep-cloning the entire destructuring subtree on every destructuring
assignment even though only a read is needed. Separately, two sites express a
plain `if` through the `.then(...).flatten()` idiom: `current_scope`
(collector.rs:50-54) — `(!has_issues).then(|| stack.last().copied()).flatten()`
— and the `ReturnedObject` candidate in `classify_candidates`
(classification.rs:205-211), obscuring the materially different
unknown/returned outcomes.

**Recommendation:** Delete the clone by changing `collect_assignment_aliases`
to accept `&AssignTargetPat` and projecting the borrowed pattern — swc exposes
no `&AssignTargetPat` → `&Pat` coercion (only a by-value `From<AssignTargetPat>
for Pat`), so the borrowing must be threaded through the projection entry
(`project_destructuring`), not done at the call site. Rewrite both
`.then(...).flatten()` sites as explicit `if` expressions. Guardrail: preserve
the exact gating — `current_scope` must return `None` (not an empty stack
fallback) once issues exist, and the `ReturnedObject` candidate must skip the
call when the rooted path is a root, exactly as the current predicate encodes.

**Fix Applied:** None so far.

#### [ ] READ-009 — Field visibility wider than the owning type; redundant hand-written `Debug`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/scope/build/program.rs:6-9`, `glass-lint-core/src/analysis/scope/build/analysis/classification.rs:24-43`

`ScopedProgram` (program.rs:6-9) is `pub(in crate::analysis)` but declares its
`graph` and `issues` fields `pub(crate)`, exposing them more broadly than the
type itself and inviting external code inside `analysis` to read the sealed
fields. `DeclarationClassification` (classification.rs:24-43) carries a
hand-written `Debug` impl that exactly mirrors what
`#[derive(Debug)]` would emit (every member — `String`, `SmolStr`,
`BindingProvenance`, `NamePath` — already derives `Debug`).

**Recommendation:** Narrow the `ScopedProgram` fields from `pub(crate)` to
`pub(in crate::analysis)` (matching the struct's own visibility) so
`graph` / `issues` are only reachable inside `analysis`. Replace the manual
`Debug` on `DeclarationClassification` with a derive.
Guardrail: the struct is destructured at scope/mod.rs:59, constructed at
freeze.rs:54, and read at build/tests.rs:248-249 — all within `crate::analysis`,
so the narrowed visibility keeps every call site working, and keep the field
names (`graph`, `issues`) stable for the destructure.

**Fix Applied:** None so far.

### Panic review

#### [ ] READ-010 — Invariant `expect`/`unreachable!` panics on restore paths that already model failure

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/scope/build/history.rs:185-206`, `glass-lint-core/src/analysis/scope/build/assignments/control_flow.rs:221-245`

`apply_assignment_inverse` panics with `expect("assignment scope must exist
while undoing")` at history.rs:192 and 197 when the delta log and the live
`HashMap` disagree, and `break_exit` uses `unreachable!("breakable frame was
checked above")` at assignments/control_flow.rs:240. The surrounding design
already models restore failure as `HistoryRestoreError` (history.rs:35-38) and
converts it into an `InvalidCheckpoint` issue (assignments.rs:240-244); these
panics are the only places where a desync aborts the process instead of
producing an issue. AGENTS.md requires expected errors to be modeled, not
panicked.

**Recommendation:** Either surface the delta-application failure through the
existing `HistoryRestoreError` channel — have `apply_assignment_inverse` signal
the missing-scope case via a flag captured by the `restore` closure (the
`ParentLinkedHistory::transition` closure returns `()`, so it cannot `return
false` directly) and convert that flag into a restore error in
`OwnedHistory::transition`, so a history/live-map desync degrades to the
already-handled `InvalidCheckpoint` path — or document these as deliberate
internal invariants with an explicit comment. Guardrail: do not relax the
invariant itself — the
delta log and live map must stay consistent under normal operation, and any
change must keep the undo/redo LCA transition behavior (history.rs:7-10)
byte-for-byte the same.

**Fix Applied:** None so far.

## Systemic Themes

- **Charge-then-intern-then-record-exhaustion** is the repeated unit across the
  chunk: `budget.try_charge()`, `names.intern/lookup`, `name_exhausted = true`
  on failure, then a `LexicalScopes` write. It appears in `plan.rs::insert`,
  `collector.rs::register_binding` (and `update_binding`'s provenance-string
  interning — though `update_binding` itself resolves the name with the
  non-interning `name_id`), `visitor.rs`
  (`record_pending_function_name`, `record_function_call`, `after_fn_decl`),
  and `plan.rs::visit_ident`/`visit_member_expr`/`visit_prop_name` — a
  candidate for one helper plus one search signal (`name_exhausted = true`).
- **Fail-closed collection** is consistently honored and should be preserved
  wherever consolidations touch it: `has_issues()` gating in `current_scope`,
  `binding_scope`, and `is_unbound`; `UnconsumedShape`/`ShapeMismatch`/
  `InvalidCheckpoint` recording; `take_child` rejection instead of fallback
  scope allocation; `scope_shape_valid` derived from empty issues (freeze.rs:42).
- **Two-pass planner/collector parallelism** intentionally splits declaration
  registration from source-order collection; findings READ-002 and READ-007
  consolidate within each pass but must not merge the two passes' lifecycles.

## Open Questions

- **`OwnedHistory<D>` vs flow `MutationLog`:** `build/history.rs:52-87`
  acknowledges it uses the same parent-linked-history approach as
  `flow/projector/history.rs:35-88`. The two add different guardrails (owner
  tagging + generic deltas vs internal budget bounds), so a shared helper is
  plausible but speculative; a consolidation would cross subsystem boundaries
  and must preserve each invariant. Not resolvable from code alone; left open
  rather than reported.
- **`BindingFreezeInput`** (binding_index.rs:35-41) is constructed once
  (freeze.rs:31-37) and consumed once (`BindingIndex::from_freeze_input`,
  binding_index.rs:60). Resolved: keep the bundle. It is `pub(super)`, consumed
  atomically by one constructor, and matches the freeze-bundle convention the
  codebase already uses (`ScopePlan`, `FrozenScopeCollectionArtifacts`); a
  five-argument signature would not remove real ceremony.
- **`FrozenScopeCollectionArtifacts`** (build/mod.rs:105-109) is produced by
  `seal()` and immediately destructured by `freeze()`. Resolved: keep it. The
  seal/destructure split is the consuming-bundle pattern, and the nested
  `FrozenPropertyArtifacts` is a real cross-module boundary value consumed by
  `ScopeGraph::finish_collected_properties` (graph.rs:176-180); collapsing the
  outer bundle would only push the destructure into `freeze()`.
- **`collect_compact_binding_names` (callbacks.rs:268-281) vs
  `for_each_pat_binding` (bindings.rs:91-97) vs `project_destructuring`
  (projection.rs:37-76):** Resolved: keep them distinct. They walk different
  inputs (`CompactPat` vs `Pat`) for different outputs (deduplicated binding
  names vs projected `(name, NamePath)` pairs), and `project_destructuring`
  alone threads `append_segment` plus the `is_assignment` flag.

## Coverage

Reviewed all `glass-lint-core/src/analysis/scope/build/*` modules:
`assignments.rs` (+ `assignments/control_flow.rs`), `bindings.rs`,
`callbacks.rs`, `collector.rs`, `compact_pat.rs`, `constants.rs`,
`freeze.rs`, `history.rs` (+ `history/tests.rs`), `plan.rs`, `program.rs`,
`projection.rs`, `provenance.rs`, `shape.rs`, `traversal.rs`, `visitor.rs`,
`aliases.rs`, `analysis/{mod,assignment,classification}.rs` (+ `analysis/tests.rs`),
`tests.rs`, `tests_extended.rs`, and `mod.rs`. Downstream consumers checked for
behavior and boundaries: `scope/mod.rs`, `scope/graph.rs`,
`scope/binding_index.rs`, `scope/mutation_index.rs`, `scope/query/*`,
`analysis/model/scope.rs`, `analysis/model/scope/provenance.rs`,
`flow/projector/history.rs`, and `analysis/semantic/budget.rs`. `git status
--short` confirms only this audit file is new; no source files were modified.
