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

- `ScopeCollector` defines every control-flow/scope operation twice — once as
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

#### [ ] READ-001 — `ScopePass for ScopeCollector` is a pure forwarding shim duplicating every inherent control-flow method

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/build/visitor.rs:32-118`

`impl ScopePass for ScopeCollector` (visitor.rs:32-118) forwards 21 methods —
`push_scope`, `pop_scope`, `current_scope`, `enter_if`, `enter_else`,
`exit_if`, `enter_loop`, `exit_loop`, `enter_switch`, `enter_switch_case`,
`exit_switch_case`, `exit_switch`, `enter_try`, `enter_catch`, `exit_try`,
`enter_function`, `exit_function`, `mark_unreachable`, `break_exit`,
`continue_exit`, `is_budget_exhausted` — whose signatures are identical to the
inherent methods in `control_flow.rs:6-291` and `collector.rs:50-54,
176-207`. Each forwarder is `fn enter_if(&mut self) { self.enter_if(); }`,
i.e. two definitions of the same operation with the same signature (the
inherent method silently shadows the trait one). Any change to control-flow
joining must now be written and maintained in two places, and the two sets can
diverge silently. This is the single largest mechanical duplication in the
chunk (~90 lines).

**Recommendation:** Make the inherent definitions in `control_flow.rs` and
`collector.rs` the trait implementation (move the bodies into
`impl ScopePass for ScopeCollector`) and delete the forwarding block in
visitor.rs, or keep the inherent set and implement the trait there. Update the
few direct inherent calls in `tests.rs` / `tests_extended.rs` to go through
the trait or `ScopeTraversal`. Guardrail: keep `ScopeEntry::Rejected`
semantics, the `ScopeStackUnderflow` / `ShapeMismatch` issue recording, and
the fail-closed `current_scope()` returning `None` after an issue; do not
apply this consolidation to `ScopePlanner`, whose trait methods are genuine
adapters over differently-typed inherent methods (plan.rs:148-167).

**Fix Applied:** None so far.

#### [ ] READ-002 — Declaration registration and `var` binding-scope selection are duplicated across planner and collector

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/build/plan.rs:93-124`, `glass-lint-core/src/analysis/scope/build/collector.rs:56-174`

`ScopePlanner::insert` / `insert_local` / `insert_import` / `insert_pat_locals`
(plan.rs:93-117) and `ScopeCollector::register_binding` / `register_local` /
`register_pat_locals` / `update_binding` / `reset_pat_locals`
(collector.rs:66-174) re-implement the same sequence — charge the semantic
budget, intern (or look up) the name, set `name_exhausted` on failure, then
`LexicalScopes::get_mut(scope).insert_binding(...)` — on the same underlying
types. `binding_scope` is likewise duplicated (plan.rs:119-124 vs
collector.rs:56-64) with subtly different behavior: the planner version does
not guard on collected issues. The exhausted-flag convention is re-encoded in
each function, so a change to budget/exhaustion handling must touch both
passes.

**Recommendation:** Extract one shared declaration-registration helper over the
owning state (`&mut LexicalScopes`, `&mut NameTable`, `&mut bool
name_exhausted`, `&SemanticBudget`) that both passes call, and centralize the
`binding_scope` `VarDeclKind::Var` rule next to the existing
`var_binding_scope` in `bindings.rs`. Guardrail: the collector variant must
keep interning provenance strings (`intern_provenance_strings`) and must
return `None` when `artifacts.has_issues()` so invalid collection fails
closed; the planner variant must not gain that guard.

**Fix Applied:** None so far.

#### [ ] READ-003 — Three name-lookup methods expose overlapping, inconsistently named surfaces

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
`name_id`. Either inline `require_module_expr_name` into its two call sites
with the explicit `ModuleRequestPolicy::alias()`, or give the module-request
helper a single well-documented entry point. Guardrail: keep
`lookup_or_intern_name` (which interns) distinct from `name_id` (which does
not) — that difference is real and load-bearing for budget accounting.

**Fix Applied:** None so far.

#### [ ] READ-005 — Assignment recording duplicates the versioning-and-push tail across two variants

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
`record_alternatives`/`record_unknown` are already about. Both are called from
the same join path (assignments.rs:279-281), so the shared tail is genuinely
common, not coincidental.

**Recommendation:** Extract a private tail such as
`push_assignment(span, scope, name, version, alternatives)` that owns
`next_assignment_version` + environment write + `assignments.push`, and have
both callers pass the concrete `ProvenanceAlternatives` they already compute.
Guardrail: keep the exact environment-write order — the join must write
`record_unknown` (not empty) when no complete witness exists so unknown stays
distinct from successful-empty, and the versions must remain strictly
increasing in source order.

**Fix Applied:** None so far.

### Pattern projection and alias collection

#### [ ] READ-004 — Declaration and assignment alias projection are near-identical wrappers

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

**Fix Applied:** None so far.

### Visitor completion hooks

#### [ ] READ-007 — `after_function` and `after_arrow` duplicate the pending-name and inline-parameter install sequence

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
(`Self::function_parameters(&function.function)` vs `Self::arrow_parameters`)
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

**Fix Applied:** None so far.

### Encapsulation and data-shape issues

#### [ ] READ-006 — Joined path assignments are a raw 3-tuple that callers destructure and re-pass field by field

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

**Fix Applied:** None so far.

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

**Recommendation:** Pass `&*pattern` (or change `collect_assignment_aliases` to
take `&AssignTargetPat`) and delete the clone. Rewrite both
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

**Recommendation:** Narrow the `ScopedProgram` fields to
`pub(in crate::analysis)` (or keep the struct private to `build` and expose
accessors) so consumers use `graph`/`issues` through the owning module.
Replace the manual `Debug` on `DeclarationClassification` with a derive.
Guardrail: `ScopedProgram` is destructured at scope/mod.rs:59 and freeze.rs:54
— keep those call sites working via the same visibility the struct uses, and
keep the field names (`graph`, `issues`) stable for the destructure.

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
checked above")` at control_flow.rs:240. The surrounding design already models
restore failure as `HistoryRestoreError` (history.rs:36-38) and converts it
into an `InvalidCheckpoint` issue (assignments.rs:240-244); these panics are
the only places where a desync aborts the process instead of producing an
issue. AGENTS.md requires expected errors to be modeled, not panicked.

**Recommendation:** Either route the delta-application failures through the
existing `HistoryRestoreError` channel (return `false` from the apply closure)
so a history/live-map desync degrades to the already-handled
`InvalidCheckpoint` path, or document these as deliberate internal invariants
with an explicit comment. Guardrail: do not relax the invariant itself — the
delta log and live map must stay consistent under normal operation, and any
change must keep the undo/redo LCA transition behavior (history.rs:7-10)
byte-for-byte the same.

**Fix Applied:** None so far.

## Systemic Themes

- **Charge-then-intern-then-record-exhaustion** is the repeated unit across the
  chunk: `budget.try_charge()`, `names.intern/lookup`, `name_exhausted = true`
  on failure, then a `LexicalScopes` write. It appears in `plan.rs::insert`,
  `collector.rs::register_binding`/`update_binding`, `visitor.rs`
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

- **`OwnedHistory<D>` vs flow `MutationLog`:** `build/history.rs:53-87`
  acknowledges it uses the same parent-linked-history approach as
  `flow/projector/history.rs:35-88`. The two add different guardrails (owner
  tagging + generic deltas vs internal budget bounds), so a shared helper is
  plausible but speculative; a consolidation would cross subsystem boundaries
  and must preserve each invariant. Left open rather than reported.
- **`BindingFreezeInput`** (binding_index.rs:35-41) is constructed once
  (freeze.rs:31) and consumed once (`BindingIndex::from_freeze_input`). It is a
  genuine cross-module freeze bundle, but it may be more ceremony than a
  signature with five arguments would be; whether it earns its own type is a
  judgment call that depends on future freeze inputs.
- **`FrozenScopeCollectionArtifacts`** (build/mod.rs:105-115) is produced by
  `seal()` and immediately destructured by `freeze()`; it adds naming but no
  invariant. Low-value cleanup only if the seal/destructure split is removed.
- **`collect_compact_binding_names` (callbacks.rs:268-281) vs
  `for_each_pat_binding` (bindings.rs:91-97) vs `project_destructuring`
  (projection.rs:37-76):** three pattern walkers with different outputs; each
  appears justified, but they should be kept distinct rather than merged.

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
