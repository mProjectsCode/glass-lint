# Codebase Readability Audit

## Summary

This audit covers Chunk 1 ("Source fact construction") of `glass-lint-core`:
the private semantic-analysis coordinator (`analysis/mod.rs`), the fact
construction pipeline (`analysis/facts/*` and `analysis/facts/calls/*`,
including `reads.rs`), and the retained fact model (`analysis/model/fact.rs`).
It is read-only; no source was modified. Only
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_01.md` was created; the pre-existing
`CODEBASE_READABILITY_AUDIT_CORE_CHUNK_*.md` files from parallel sessions were
left untouched.

The chunk is well-structured overall. Provenance is centralized in
`OriginChannels` with deliberate asymmetric per-map checkpoint/snapshot
semantics; `OriginMap` (chunk 2) provides the log-based rollback primitive;
the visitor owns all traversal and never leaks SWC types. No provider policy
or public-API leakage was found (`FactBuilder`, `FactStream`, `BuiltFacts`,
`SemanticFacts`, and all provenance types are `pub(in crate::analysis)` and
nothing in `lib.rs` re-exports them).

The findings below are mostly about indirection and parallel-code families
accumulated while the pipeline grew: a one-field provenance façade, duplicated
wrapper detection, repeatedly hand-inlined TypeScript assertion unwrap arms,
two nearly identical callee fallback arms, three identical derived-phase
accessors, a transient construction-metadata relay struct, duplicated
function-scope lookups, 36 identical budget guards, a duplicated dynamic
import special case, and a parallel `ResolvedCallee`/`CallEvent` field copy.

## Findings

### Fact construction and provenance

#### [ ] READ-001 — `FactProvenanceState` is a one-field façade over `OriginChannels`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/provenance.rs:257-408`

`FactProvenanceState` wraps a single `origins: OriginChannels` field and
forwards 18 of its 20 methods verbatim (checkpoint, every restore/retain/
finish operation, and every single-map accessor). Only `branch_provenance`
(302-310) and `replace_targets` (398-407) add behavior, and both belong
naturally on `OriginChannels`. Every caller (`FactBuilder::provenance` at
`facts/mod.rs:92`, plus ~30 call sites in `control.rs`, `callee.rs`,
`construction.rs`, `assignments.rs`, `visitor.rs`) goes through this layer,
so any future change to the channel set must be threaded through twice.

**Recommendation:** Delete `FactProvenanceState`; make `FactBuilder::provenance`
hold `OriginChannels` directly and move `branch_provenance` and
`replace_targets` onto `OriginChannels`. Guardrails: keep the four channels,
the asymmetric per-map lifecycle semantics, and the `ProvenanceCheckpoint` /
`TargetProvenance` / `InstanceProvenanceSnapshot` / `BranchProvenance` types
intact; this finding removes one indirection layer, it does not merge maps or
change control-flow join behavior.

**Fix Applied:** None so far.

#### [ ] READ-002 — `.call`/`.apply` wrapper-member detection duplicated across two files

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/calls/mod.rs:44-52`, `glass-lint-core/src/analysis/facts/visitor.rs:109-126`

`record_call_expr` (calls/mod.rs:44-52) and `visit_opt_chain_expr`
(visitor.rs:109-126) both derive an optional `&MemberExpr` wrapper by
"effective callee is a member whose literal property is `call` or `apply`",
then feed the identical `(span, callee_expr, args, wrapper)` tuple to
`record_call_like`. The predicate (same `matches!` on
`literal_member_property_name(...).as_deref()`) is structural and single
concept; the optional-chain arm in the visitor only adds one extra level of
`OptChain` unwrapping before the same check.

**Recommendation:** Extract `call_apply_wrapper(callee: &Expr) -> Option<&MemberExpr>`
(owner: `analysis::syntax` alongside `effective_callee_expr`, or
`facts::calls::wrapper`), and call it from both sites so wrapper recognition
lives in exactly one place. Guardrails: the visitor's nested-`OptChain` lookup
must be preserved, and the caller that found a wrapper must keep calling
`record_call_like` with `Some(member)` so `try_emit_callable_wrapper_common`
and evidence order are unchanged.

**Fix Applied:** None so far.

#### [ ] READ-003 — TypeScript-assertion + sequence + paren unwrap arms repeated by hand in four resolvers

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/calls/callee.rs:190-199`, `glass-lint-core/src/analysis/facts/calls/callee.rs:221-232`, `glass-lint-core/src/analysis/facts/functions.rs:282-296`, `glass-lint-core/src/analysis/facts/arguments.rs:112-115`, `glass-lint-core/src/analysis/facts/arguments.rs:176-184`

The identical six-arm recursion
`Paren -> Seq(last) -> TsAs -> TsNonNull -> TsSatisfies -> TsTypeAssertion`
is written out in `instance_origin_for_expr` (callee.rs:190-199),
`constructor_origin_for_expr` (callee.rs:221-232), and `class_operand_name`
(functions.rs:282-296); `member_chain_projection` and `analyze_argument_tree`
(arguments.rs:112-115, 176-184) repeat the `Paren`/`Seq` subset. Each new
TypeScript wrapper or call site re-expresses the same transparency rule, and
the arm order must stay in sync across all five functions.

**Recommendation:** Add one `analysis::syntax` helper that unwraps
Paren/Seq/TsAs/TsNonNull/TsSatisfies/TsTypeAssertion to the innermost
expression (or extend `effective_callee_expr`'s transparency set, keeping its
callers unchanged), and collapse each resolver's wrapper cluster into a single
arm. Guardrails: `Member` and other terminals must remain opaque here
(sequence "last-expression" semantics and TS-assertion transparency are the
only behaviors being consolidated); do not route these through the contextual
constant evaluator, which changes accepted shapes.

**Fix Applied:** None so far.

#### [ ] READ-004 — `resolve_call_callee` contains two byte-identical fallback arms

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/calls/callee.rs:79-101`

The `Expr::OptChain` arm's non-member branch (callee.rs:79-91, the part after
the `OptChainBase::Member` early return) and the `_` fallback arm
(callee.rs:92-101) both perform `resolve_expr(effective)` +
`byte_range(effective.span())?` + `function_id_for_expr(effective)` +
`ResolvedCallee::from_resolved(...)`. Any change to the fallback lowering has
to be applied twice.

**Recommendation:** Hoist the shared fallback into one arm covering
`OptChain(non-member base) | _` after the `Ident`/`Member`/`OptChain(Member)`
cases have returned. Guardrails: keep `effective` (the `effective_callee_expr`
result) as the single operand and the early `OptChainBase::Member` return so
member resolution and its `receiver`/`syntactic_path` population are unchanged.

**Fix Applied:** None so far.

#### [ ] READ-005 — `DerivedPhaseCapabilities` exposes three identical per-phase accessors over one flag

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/mod.rs:55-82`

`DerivedPhaseCapabilities` stores one `DerivedPhaseAvailability` and forwards
it through three named accessors (`export_origins`, `fact_index`, `effects`,
mod.rs:71-81) whose bodies are identical; the doc comment at mod.rs:52-54
explicitly states the design is all-or-nothing with no per-phase granularity.
Defining three phase-named getters for a value that cannot currently vary per
phase is speculative API that callers (`semantic/mod.rs:338,383`,
`facts/mod.rs:388`, `local.rs:407`) must coordinate by hand.

**Recommendation:** Until an independent per-phase disable exists, replace the
three accessors with a single `availability()` accessor (keeping the
all-or-nothing doc invariant), or thread the plain `DerivedPhaseAvailability`
value through and delete the wrapper. Guardrails: preserve the documented
"reintroduce per-phase granularity only when an independent disable is added"
note, and the fail-closed behavior where incomplete analysis disables every
derived phase together.

**Fix Applied:** None so far.

#### [ ] READ-006 — `ConstructionMetadata` is a one-call relay struct between two phases

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/construction.rs:12-17`, `glass-lint-core/src/analysis/facts/visitor.rs:135-142`

`resolve_construction_metadata` (construction.rs:20-72) packs four resolved
values into `ConstructionMetadata`, `visit_construction_children`
(construction.rs:74-76) runs between, and `emit_construction_fact`
(construction.rs:78-96) immediately destructures the struct; the only caller is
`visit_new_expr` (visitor.rs:135-142). The struct adds no invariant or
vocabulary beyond a grouped return; the two-phase split exists only so the
children are visited before the construction fact is emitted.

**Recommendation:** Collapse the three calls into one
`record_new_expr(&mut self, new_expr)` method that resolves metadata, visits
children, and emits (matching the existing `record_assignment` /
`record_call_expr` naming), deleting `ConstructionMetadata`. Guardrails: keep
the child visit strictly between resolution and emission so provenance
recording and deterministic evidence order are unchanged.

**Fix Applied:** None so far.

#### [ ] READ-007 — Duplicated function-scope lookup and `set_function` in enter/exit facts

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/functions.rs:77-121`

`emit_function_fact` (functions.rs:82-86) and `emit_function_exit_fact`
(functions.rs:109-113) both run the identical `scope_at(span)?` +
`function_scope_at(scope)` + `traversal.set_function(id)` sequence before
their asymmetric payload work (Enter registers parameter bindings; Exit is a
pure marker). `record_function_body` always invokes both with the same span.

**Recommendation:** Extract a `set_function_at(&mut self, span) -> Option<FunctionId>`
helper used by both emitters. Guardrails: keep `register_function_parameters`
and the `Enter`/`Exit` payload difference strictly in the two emitters; `Exit`
must remain free of parameter data per the flow-marker contract.

**Fix Applied:** None so far.

#### [ ] READ-008 — 36 identical budget-exhausted guards in the visitor

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/visitor.rs:24-395`

Every one of the 36 overridden visit methods opens with the same three-line
`if self.resolver.budget().exhausted() { return; }` (offsets at
visitor.rs:24,42,52,... through 395), a fail-closed invariant that any future
visit override must remember to repeat. New SWC node handlers will silently
skip the guard without a compiler signal.

**Recommendation:** Define a single guarded-dispatch helper or small macro
(e.g. `budget_guard!()` at the top of each visit body, or a `BudgetChecked`
wrapper around the visitor methods) so the guard is declared once and can't be
forgotten. Guardrails: the guard must keep running before any resolver or
provenance access so exhausted budgets produce no partial mutation; it is a
semantic invariant, not a cosmetic early return.

**Fix Applied:** None so far.

### Call lowering and model

#### [ ] READ-009 — Dynamic-import `Callee::Import` special case written twice

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/calls/mod.rs:22-27`, `glass-lint-core/src/analysis/facts/calls/mod.rs:129-141`

`record_call_expr` (calls/mod.rs:23) and `value_for_expr`
(calls/mod.rs:132) both test `matches!(call.callee, Callee::Import(_))` to
select between `resolve_expr_id` and the `CallResultTable` path. The rule
"dynamic imports do not get a fresh call-result identity" appears in two
places, and `module_request.rs:99` re-states the same predicate for a
different decision.

**Recommendation:** Extract `is_dynamic_import(call: &CallExpr) -> bool` into
`analysis::syntax` (or the module-request module) and call it from both fact
paths. Guardrails: keep `Callee::Super`/`Import` behavior intact — the 
`call_result` identity path must not start producing values for imports.

**Fix Applied:** None so far.

#### [ ] READ-010 — `ResolvedCallee` and `CallEvent` are parallel 13-field records with a hand-written field copy

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/facts/calls/callee.rs:15-29`, `glass-lint-core/src/analysis/model/fact.rs:238-254`, `glass-lint-core/src/analysis/model/fact.rs:353-383`

`ResolvedCallee` (13 fields, all `pub(in crate::analysis)`) and `CallEvent`
(13+ fields, private with accessors) duplicate nine fields with identical
names; `into_call_event` (fact.rs:353-383) transcribes them by hand while
`from_resolved` + three post-construction mutations (`callee.callee_name = ...`,
`callee.syntactic_path = ...`, `callee.instance_class = ...`, callee.rs:73-75,
131-133) build the source record. Two lowering styles also coexist:
`CallEvent::unknown` (fact.rs:257) for the non-expr-callee path versus
`into_call_event` for resolved callees.

**Recommendation:** Capture the callee-owned field set once, either as a
`From<ResolvedCallee>`-style conversion for the shared substructure appended by
a small builder for the derived fields (`result`, `rooted_chain`,
`returned_member`, `args`, `unwrap`), or by folding the three field mutations
into one `ResolvedCallee` constructor plus one lowering entry point. Guardrails:
keep the resolver/interner out of the model (fact.rs must stay free of resolver
access, as it is today), keep `CallEvent` fields behind semantic accessors, and
preserve the distinction that `CallEvent::unknown` represents an
unresolvable/import callee rather than a resolved one.

**Fix Applied:** None so far.

## Systemic Themes

- **Hand-enumerated per-map provenance bookkeeping.** Every lifecycle operation
  in `OriginChannels` (restore/finish/rollback/commit/retain/replace,
  `provenance.rs:56-197`) lists the same four maps in the same order, and
  `BranchProvenance`/`InstanceProvenanceSnapshot` nest three more snapshots
  beneath them. The heterogeneous value types (`ClassIdentity`,
  `InstanceCallable`, `ByteRange`) prevent a homogeneous loop, and the semantics
  are intentionally asymmetric (documented at provenance.rs:10-12), so this is a
  coherent design rather than a defect; a small macro or tuple-typed helper
  would remove the ordering obligation if the base abstraction in chunk 2's
  `origin_map.rs` grows more operations.
- **Fairly broad but well-documented `FactBuilder` method surface.** Splitting
  one `impl` across nine modules (`arguments`, `assignments`, `calls/*`,
  `construction`, `control`, `functions`, `reads`) keeps each concern cohesive
  and all traversal state on the single owning type; the cost is that
  sub-modules reach unidirectionally into `resolver`/`provenance` internals.
  This is consistent with the `analysis` doc ("Put behavior on the type that
  owns the state") and is noted only as a growth risk.
- **Sentinel `FunctionId::new(0)` for "top level".** `emit` (mod.rs:203-207)
  compares `current_function() == FunctionId::new(0)` to decide whether to fall
  back to a scope lookup; the sentinel is not named and is re-derived by
  `scope_at`/`function_scope_at` in `functions.rs`. A named constant or a
  `TraversalState::function_for(scope)` owner would clarify the convention.

## Open Questions

1. Is `ResolvedCallee` consumed anywhere after `into_call_event` (specifically
   its `bound_arguments`), or is it strictly transient? If transient, READ-010
   could collapse more aggressively rather than only deduplicating the field map.
2. The `DerivedPhaseCapabilities` per-phase accessors are asserted individually
   in tests (semantic/tests.rs:75-77). Are they scaffolding for an imminent
   independent-phase disable, or a historical artifact? This determines whether
   READ-005 deletes the wrapper or just the two redundant accessors.
3. `record_call_expr`'s wrapper detection (calls/mod.rs:44-52) uses
   `effective_callee_expr` while `visit_opt_chain_expr` (visitor.rs:109-117)
   first unwraps a nested `OptChain` before the same check. Can the two paths
   be unified inside `record_call_like` with one callee-wrapping rule, or must
   the optional-chain path stay separate because its callee can itself be a
   chain? This bears on READ-002 and was left unresolved.
4. Whether the four-map provenance operations (systemic theme 1) merit a macro
   depends on the `OriginMap` abstraction's future; the base-layer decision
   belongs to the chunk 2 audit of `origin_map.rs`.

## Coverage

Files read in full and cited in findings:

- `glass-lint-core/src/analysis/mod.rs`
- `glass-lint-core/src/analysis/facts/mod.rs`
- `glass-lint-core/src/analysis/facts/provenance.rs`
- `glass-lint-core/src/analysis/facts/origin_map.rs` (supporting base layer; chunk 2 scope)
- `glass-lint-core/src/analysis/facts/arguments.rs`
- `glass-lint-core/src/analysis/facts/assignments.rs`
- `glass-lint-core/src/analysis/facts/call_results.rs`
- `glass-lint-core/src/analysis/facts/calls/mod.rs`
- `glass-lint-core/src/analysis/facts/calls/callee.rs`
- `glass-lint-core/src/analysis/facts/calls/wrapper.rs`
- `glass-lint-core/src/analysis/facts/construction.rs`
- `glass-lint-core/src/analysis/facts/control.rs`
- `glass-lint-core/src/analysis/facts/functions.rs`
- `glass-lint-core/src/analysis/facts/instance.rs`
- `glass-lint-core/src/analysis/facts/reads.rs`
- `glass-lint-core/src/analysis/facts/state.rs` (supporting; chunk 2 scope)
- `glass-lint-core/src/analysis/facts/visitor.rs`
- `glass-lint-core/src/analysis/model/fact.rs`
- `glass-lint-core/src/analysis/syntax/names.rs` (reused transparency helpers)

Callers/references traced via `rg`: `SemanticFacts`/`BuiltFacts`/`from_analysis`
through `analysis/semantic/mod.rs`, `analysis/local.rs`, `project/projection.rs`,
`project/model.rs`, `matching/*`, and `flow/*`; `DerivedPhaseCapabilities`
through `semantic/mod.rs` and `local.rs`; provenance accessors and
fact-builder methods through `control.rs`, `calls/*`, `construction.rs`,
`assignments.rs`, and `visitor.rs`; `lib.rs` public surface confirmed to expose
none of the chunk-1 types.