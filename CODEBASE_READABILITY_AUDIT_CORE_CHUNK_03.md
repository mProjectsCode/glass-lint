# Codebase Readability Audit

## Summary

Chunk 3 — scope collection frontend (`analysis::scope` entry point,
`binding_index` freeze boundary, `build` orchestration state, `aliases`, and
the `build::analysis` classifier/assignment-effect helpers). The collection
state types (`LexicalCollectionState`, `FunctionCollectionState`,
`PathCollectionState`, the `*Checkpoint` cluster) are cohesive path-local
owners with real reversible-history invariants; the audit does not find them
accidentally split. The concrete issues are one collected-output field living
in the wrong owner, an immediately-consumed duplicate "frozen" artifacts type,
an over-machined binding-index freeze boundary built around a structurally
unreachable unit error, a caller-side free-function chain for assignment
effects that duplicates `provenance.rs` composition, and one provably dead
classification candidate.

5 findings: READ-001 — READ-005.

## Findings

### Scope collection frontend

#### [x] READ-002 — `FrozenScopeCollectionArtifacts` and `ScopeCollectionArtifacts::seal` are an immediately-consumed duplicate type

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/scope/build/mod.rs:52-109`, `glass-lint-core/src/analysis/scope/build/freeze.rs:17-23`

`ScopeCollectionArtifacts` (mod.rs:53-60) and `FrozenScopeCollectionArtifacts`
(mod.rs:103-109) are two structs with the identical five private fields; the
only consumer of `seal()` (mod.rs:91-100) is `freeze.rs:23`
(`std::mem::take(&mut self.artifacts).seal()`), which destructures the frozen
struct back into the same five parts in the very next statement (freeze.rs:
17-23). The "frozen" type is consumed immediately, adds no invariant beyond the
`consume(self)` that moving the fields directly would already enforce. Its
`Default` derives serve that `mem::take` and, on `ScopeCollectionArtifacts`
(mod.rs:53), also initialize the field at `collector.rs:41`.

**Recommendation:** Delete `FrozenScopeCollectionArtifacts` and `seal()`;
destructure the `ScopeCollectionArtifacts` fields directly in
`ScopeCollector::freeze`. The deletion is ownership-clean: `freeze` takes
`mut self` by value, so destructuring `self.artifacts` partial-moves only the
artifacts and leaves `lexical`/`functions`/`assignment` usable (freeze.rs:24-40).
Consolidate the record/issue accessors on the one remaining type. Guardrails:
preserve consume-once semantics (freeze still owns the artifacts), the
capture-then-consume ordering, and the exact `Issues`/fact split feeding
`ScopeGraph::from_collected` (freeze.rs:46-60). Implementation order: after
READ-001, which adds the `assignments` field to the same struct — purely
stylistic, since `assignments` does not participate in the seal/destructure
bundle.

**Fix Applied:** Removed the immediately-consumed frozen wrapper and `seal`
method. `ScopeCollector::freeze` now destructures its owned
`ScopeCollectionArtifacts` directly; field capture order and downstream scope
graph construction remain unchanged.

#### [x] READ-001 — `AssignmentCollectionState.assignments` stores a collected output inside the reversible traversal-state owner

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/scope/build/mod.rs:156-162`, `glass-lint-core/src/analysis/scope/build/assignments.rs:131-159`, `glass-lint-core/src/analysis/scope/build/freeze.rs:33-35`

Every collected output in the collector flows into one owner,
`ScopeCollector.artifacts: ScopeCollectionArtifacts` (mod.rs:53-60,
mod.rs:180-193). The exceptions are the source-order assignment fact list and
the call list: `AssignmentCollectionState.assignments: Vec<AliasAssignment>`
(mod.rs:157-162) mixes that finalized output with the mutable per-path state
(`version_counters`, `path: PathCollectionState`), and
`FunctionCollectionState.calls: Vec<FunctionCall>` (mod.rs:151) is a
same-shaped collected output outside artifacts — consumed only at freeze via
`parameter_aliases()` (visitor.rs:640; callbacks.rs:24-67), never read or
rolled back mid-traversal. `push_assignment` (assignments.rs:131-159) writes
both the output list and the reversible environment in one step, and
`freeze.rs:33-35` reaches into `self.assignment.assignments` to extract it. One
invariant — "collected facts live in artifacts" — is split across owners, so
callers must know the internal shape of traverser state types to silence an
output.

**Recommendation:** Move the `assignments` field into `ScopeCollectionArtifacts`
(a `Vec` plus a narrow `record_assignment_fact` method), keep `version_counters`
and `path` in `AssignmentCollectionState`, and have `push_assignment` append
through the artifact accessor while it records the environment. `freeze` then
consumes all outputs through the one owner. If the collected-facts boundary is
meant as policy, `FunctionCollectionState.calls` (mod.rs:151) needs the same
treatment; otherwise the boundary should be described as applying to
reversible per-path state. Guardrails: retain source-order determinism of the
pushed facts and the exact `AliasAssignment` construction; do not merge the
collected output with control-flow/undo state, which must stay per-path and
reversible.

**Fix Applied:** Moved assignment facts into `ScopeCollectionArtifacts` with a
narrow `record_assignment_fact` accessor. `AssignmentCollectionState` now owns
only version counters and reversible path state; freeze consumes the assignment
facts from the artifacts owner. The source-order push and exact
`AliasAssignment` construction remain unchanged. `FunctionCollectionState.calls`
was left untouched because this focused change addresses the assignment-state
boundary without broadening the refactor.

#### [x] READ-003 — Binding-index freeze boundary is over-machined around a structurally unreachable unit error

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/binding_index.rs:13-81`, `glass-lint-core/src/analysis/scope/build/freeze.rs:31-45`

The freeze transition spans `BindingFreezeInput` (binding_index.rs:15-23, a
one-use factual bundle), `BindingIndex::from_freeze_input` (binding_index.rs:
39-61), `BindingIndexError` (binding_index.rs:25-26, a payload-less unit type),
the `resolve_function_targets`/`function_for_scope` helpers (binding_index.rs:
64-81), and a two-way degrade in `freeze.rs:42-45` (`InvalidBindingIndex`
issue plus `BindingIndex::empty()`). The only failure source is
`function_ids.get(&scope)` missing a scope that was keyed as a function
binding/alias (binding_index.rs:76-81). That miss is structurally impossible in
the current pipeline: function scopes are entered with `ScopeKind::Function`
(traversal.rs:126), function bindings/aliases are recorded only for those
entered scopes, and `allocate_ids` assigns a `FunctionId` to every
`Program | Function` scope, so the map covers every function-scope key
(binding_index.rs:107-119). The
`Result` therefore carries zero discriminative information, and the fallback
(`empty()` + issue) differs from a straight filtered resolution only in how it
is framed. Neither `BindingIndexError` nor `from_freeze_input` documents the
construction contract or the invariant the error guards.

**Recommendation:** Simplify the boundary to match the real contract: either
drop the `Result`/`BindingIndexError` and let `from_freeze_input` resolve with
a single documented filter that records `InvalidBindingIndex`-style invalidity,
or — if the guard is to stay — give `BindingIndexError` a payload (the failing
scope and which map produced it) so the degrade is observable, and document the
"every function binding/alias scope has a FunctionId" invariant on both the
constructor and the type (traversal.rs:126; binding_index.rs:107-119).
Guardrails: any failure must still fail closed through the recorded issue
(never panic, per AGENTS.md), and `empty()` must remain distinguishable from a
successful empty result by the issue flag. `BindingIndex::empty()` also has a
second, unrelated consumer in the test/diagnostic empty graph (graph.rs:72), so
it must remain `pub(super)` regardless of which option is chosen. Keep
`BindingFreezeInput` as the named phase-boundary bundle. Implementation order:
after READ-002 (the same freeze block is touched).

**Fix Applied:** Kept the checked, fail-closed freeze boundary but replaced the
payload-less `BindingIndexError` with the missing `ScopeId`. The issue now
retains that scope as `InvalidBindingIndex { scope }`, and the constructor
documents the collector invariant that all function-binding targets receive
IDs during allocation. The named `BindingFreezeInput` phase bundle and empty
fallback remain unchanged.

#### [ ] READ-004 — Assignment-effect helpers are a caller-side free-function chain over `ScopeCollector` state that duplicates the provenance.rs composition

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/scope/build/analysis/assignment.rs:5-34`, `glass-lint-core/src/analysis/scope/build/analysis/mod.rs:1-9`, `glass-lint-core/src/analysis/scope/build/provenance.rs:118-137`, `glass-lint-core/src/analysis/scope/build/visitor.rs:536-555`

`assignment_provenance` (assignment.rs:21-34) and
`expression_is_mutable_static_object` (assignment.rs:5-19) are free functions
whose only argument (besides an AST node) is `&mut ScopeCollector`, and every
step delegates back to collector provers (`constructed_instance_provenance`,
`bound_callable_provenance`, `module_alias_provenance`, `returned_object`,
`const_provenance`, `rooted_name_path`, `static_object_values`). They are
precedence policies over one type's own state — the same role `provenance.rs`
already owns for argument and declaration provenance. `assignment_provenance`'s
chain (constructed → bound → module → returned → const → rooted → `Local`)
shares its prefix and suffix with `argument_provenance` (provenance.rs:118-137:
constructed → module → returned → static-object → const → rooted), and
`expression_is_mutable_static_object`'s `static_object_values || const`
provenance.rs:249-269 arm exactly repeats `Candidate::StaticObject` in
classification.rs:182-185. The free functions add a caller-side indirection
layer with no additional owner, vocabulary, or invariant.

**Recommendation:** Move `assignment_provenance` and
`expression_is_mutable_static_object` onto `ScopeCollector` as `pub(super)`
inherent methods beside the other provers in `provenance.rs`, and extract the
shared `static_object_values(expr).or_else(const_provenance(expr))` probe used
by both (`expression_is_mutable_static_object`, assignment.rs:14-16, and
`Candidate::StaticObject`, classification.rs:182-185) so `classification.rs`
consumes the same helper without reordering either chain. Chain consolidation
must be gated on the Open Question 3 resolution below (Resolved Open
Questions): the middle orderings diverge deliberately, not by drift, so only the
exactly-aligning sub-sequences — the constructed→module→returned prefix and the
const→rooted→`ValueAlias` suffix — may move into one shared collector operation,
with the element/fallback differences left explicit (`Local` fallback for
assignments, `Option` for arguments, bound-callable only on the assignment
side). Keep `classification.rs` as the declaration-role owner with its
`DeclarationClassification` vocabulary and tests. Guardrails: preserve the exact
precedence order tested in `analysis/tests.rs` (bound-callable before rooted
alias, constant after module, `Local` failure fallback) and fail-closed behavior
for dynamic values.

**Fix Applied:** None so far.

#### [x] READ-005 — `Candidate::BoundCallable` is provably dead in `classify_call`'s fallthrough candidate list

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/scope/build/analysis/classification.rs:146-158`

`classify_call` returns early with `[BoundCallable]` when
`callee_is_bind_call(call)` is true (classification.rs:136-143); the
fallthrough branch then lists `Candidate::BoundCallable` first again
(classification.rs:151-152). Confirmed on review: in the fallthrough branch the
call is a non-`bind` call, and `collector.bound_callable_provenance` returns
`None` for every non-`bind` member callee under an identical shape gate
(provenance.rs:140-152), so the candidate can never fire there. Keeping an
unreachable precedence arm in the ordered candidate list subtly misleads readers
about what collection can classify and forces an extra prober call per
`Call`-initiated declaration. Deleting only the fallthrough entry also removes
that wasted prober call with no dead-code regression, since
`Candidate::BoundCallable` remains used by the `bind`-call path
(classification.rs:136-143).

**Recommendation:** Delete `Candidate::BoundCallable` from the fallthrough list
in `classify_call`, leaving it only on the `bind`-call path; add an adversarial
unit test asserting that a non-bind call never classifies through the
bound-callable arm. Guardrails: keep `Candidate::BoundCallable` for genuine
`x.bind(...)` callee forms and preserve the remaining fallthrough order
(`ModuleAlias`, `Constant`, `ReturnedObject`, `RootedAlias`).

**Fix Applied:** Removed the unreachable `BoundCallable` candidate from the
non-bind fallthrough list. The dedicated bind-call path remains unchanged, as
do the remaining candidate precedence and fail-closed behavior.

## Systemic Themes

- **Path-local checkpoint cluster is deliberate, not accidental.** The
  `CollectorCheckpoint`/`FunctionCheckpoint`/`ControlFlowFrame`/`PathCollectionState`
  types form one cohesive reversible-state design backed by the shared
  `OwnedHistory` parent-linked mutation log (history.rs). The audit treated them
  as cohesive owners and found no split worth reporting; the `FunctionCall` /
  `PendingFunctionName` / `FunctionBinding` records are each consumed where they
  are produced and stay coherent with their owning `FunctionCollectionState`.
- **Collected facts vs. traversal state is the recurring boundary.** READ-001 is
  the reported instance, but the "only place" framing is imprecise:
  `FunctionCollectionState.calls` (mod.rs:151; pushed at visitor.rs:640,
  consumed at freeze via `parameter_aliases()`, callbacks.rs:24-67) is a
  same-shaped collected output living outside `ScopeCollectionArtifacts`. If the
  collected-facts boundary is meant as policy, `calls` needs the same treatment
  READ-001 gives `assignments`; otherwise the theme should be delimited to
  reversible per-path state (`version_counters`, `path`). Everywhere else the
  documents already follow the clean pattern of collecting into
  `ScopeCollectionArtifacts`.
- **Fail-closed by issue flag, never panic.** Freeze (READ-003) and checkpoint
  restore (assignments.rs:250-254) degrade to recorded issues plus conservative
  empty/broken state rather than panicking; any refactor must preserve that.
- **`super::` intra-build references are pervasive across submodules**
  (`collector.rs:33`, `visitor.rs:333/343/421/507/640`, `assignments.rs:24`)
  despite AGENTS.md preferring `crate::` imports. Cosmetic; consolidated only
  incidentally when a submodule's contents move.
- **Test-only instrumentation lives in the production struct** (`scope_lookups`,
  mod.rs:192) behind `#[cfg(test)]`. Acceptable; noted so it is not mistaken for
  dead code.

## Resolved Open Questions

- **Is `FrozenScopeCollectionArtifacts` intended as a future public phase
  boundary for the `build::program` freeze dialectic (i.e., a struct that later
  chunks will grow methods on)? — Resolved: no; the READ-002 deletion is safe.**
  It is `pub(super)` and method-less (mod.rs:103-109), reachable only from
  freeze.rs (freeze.rs:5, 17-23); `seal()` has exactly one caller (freeze.rs:23).
  The actual downstream immutable boundary is `graph.freeze()` →
  `FrozenScopeGraph` (graph.rs:100; scope/mod.rs:40).
- **Should `allocate_ids` (binding_index.rs:85-123) live on `BindingIndex` at
  all, or on the collector that owns `LexicalScopes`? — Resolved: keep it on
  `BindingIndex`.** Its only caller is freeze.rs:32, and it produces precisely
  the maps that `from_freeze_input` consumes (binding_index.rs:85-123;
  freeze.rs:33-41), so grouping it with `BindingIndex` keeps the freeze boundary
  self-contained. Moving it onto the collector would export the ID-iteration
  policy to callers for no current benefit; there is no separate cross-module
  freeze contract to justify a move (graph.rs:72 shows `empty()` is the only
  other binding-index entry other code touches).
- **Does the `assignment_provenance` / `argument_provenance` precedence overlap
  (READ-004) represent intentional divergence or drift? — Resolved: intentional
  divergence; the two chains must not be merged.** `bound_callable_provenance`
  appears only on the assignment/RHS chain (assignment.rs:24) because
  bound-callable is not a form the callback-argument projection consumes; the
  `ident → StaticObjectValues` step (provenance.rs:122-130) and the
  `static_object_values` probe (provenance.rs:132) exist solely to seed
  callback-parameter bindings (callbacks.rs:184, 222, 237; visitor.rs:637); and
  `const` follows `static_object_values` in the argument chain but precedes it —
  with no static-object step at all — in the assignment chain. The assignment
  order is pinned by tests (tests.rs:151-160, 247-256) while nothing pins the
  argument order, so any consolidation must preserve each chain's relative order
  and share only the agreeing segments (module→returned; const→rooted→
  `ValueAlias`).

## Coverage

Files reviewed for this chunk (Chunk 3, scope collection frontend):

- `glass-lint-core/src/analysis/scope/mod.rs` (entry point `collect_scoped_program`)
- `glass-lint-core/src/analysis/scope/binding_index.rs` (`BindingIndex`,
  `BindingIndexError`, `BindingFreezeInput`, `allocate_ids`)
- `glass-lint-core/src/analysis/scope/build/mod.rs` (all `*CollectionState`,
  `*Checkpoint`, `ScopeCollector`, `ScopeCollectionArtifacts`,
  `FrozenScopeCollectionArtifacts`, `FunctionCall`, `FunctionBinding`,
  `PendingFunctionName`, `ScopedDynamicEval`)
- `glass-lint-core/src/analysis/scope/build/freeze.rs` (freeze transition)
- `glass-lint-core/src/analysis/scope/build/aliases.rs`
- `glass-lint-core/src/analysis/scope/build/analysis/{mod.rs,classification.rs,assignment.rs,tests.rs}`
- Callers/lifecycle traced for the above types:
  `build/{collector.rs,visitor.rs,assignments.rs,callbacks.rs,history.rs,plan.rs,traversal.rs,program.rs,provenance.rs,bindings.rs,constants.rs}` and
  `scope/graph.rs`, `scope/graph/storage.rs`, `scope/query/functions.rs`,
  `scope/query/provenance/callable.rs`, `scope/query/bindings.rs`.

Read-only audit; no source files were modified.
