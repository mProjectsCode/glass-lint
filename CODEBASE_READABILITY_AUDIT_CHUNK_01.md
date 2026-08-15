# Codebase Readability Audit — glass-lint-core Chunk 1: Source fact construction

## Summary

Chunk 1 owns the source fact-construction half of the semantic pipeline in
`glass-lint-core`: `analysis` coordination (`analysis/mod.rs`, including
`DerivedPhaseAvailability`/`DerivedPhaseCapabilities`), the `facts` module tree
(`FactBuilder`, `FactStream`, `SemanticFacts`, `BuiltFacts`, `OriginMap`,
provenance, arguments, assignments, calls/callee/wrapper, construction,
control, functions, pattern, instance, interface), and the SWC visitor that
drives it. The chunk's contract with siblings is: one AST walk that emits a
deterministic, fail-closed `FactStream<Building>` plus a `ModuleInterface`
(`facts::build` -> `BuiltFacts`), frozen into `FactStream<Frozen>` and wrapped
into `SemanticFacts` (`SemanticFacts::from_analysis`) before matcher indexes,
function effects, and project linking consume it. Matcher-independent facts
must be built exactly once, evidence order must be deterministic, and
unsupported/exhausted input must stay distinct from a successful empty stream.

Overall the chunk is well-architected: the phase-typed `FactStream`, the
`FactStreamToken` construction authority, the `OriginMap` checkpoint/rollback
design, and the typed `FactStreamIssueSet` are strong, fail-closed designs. The
findings below are concrete, non-speculative improvements: three identical
member-read lowering blocks, a provenance lifecycle surface duplicated across
`OriginChannels` and `FactProvenanceState`, an inconsistent Import/Call fact
order for module calls, a capability struct whose three fields always move
together, and several small API/duplication issues.

## Findings

### Fact emission (visitor / assignments / calls)

#### [x] READ-001 — Member-read lowering is duplicated in three places

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/visitor.rs:47-59`, `glass-lint-core/src/analysis/facts/visitor.rs:164-177`, `glass-lint-core/src/analysis/facts/assignments.rs:69-80`

`visit_member_expr`, the `OptChainBase::Member` arm of `visit_opt_chain_expr`,
and `record_member_assignment` each run the identical projection+emit sequence
for a `FactPayload::MemberRead`: `resolver.resolve_member`, `member_expression_chain`,
`chain.and_then(name_path)`, `rooted_path(rooted_chain)`, `module_member.clone()`,
`returned_path(returned_member)`, then `emit(member.span(), MemberRead { .. })`.
Any change to the member-read model (a new field, a span policy, a budget
guard) must be edited in three places, and evidence order can drift if one
site changes its child-visitation.

**Recommendation:** Add one `pub(super) fn record_member_read(&mut self, member: &MemberExpr)`
on `FactBuilder` in a single module (e.g. `visitor.rs` or a small `reads.rs`)
that performs the projection and emits `FactPayload::MemberRead`; have all
three call sites invoke it. Guardrail: keep child visitation at the call
sites exactly as today (`visit_member_expr` and the opt-chain arm visit
children after the emit; `record_member_assignment` visits `obj`/`prop`
beforehand and the RHS afterwards), so evidence order is unchanged.

**Fix Applied:** Added `FactBuilder::record_member_read` in a new `reads.rs` module; `visit_member_expr`, the opt-chain `Member` arm, and `record_member_assignment` now all call it with child visitation unchanged.

#### [ ] READ-002 — Import fact order relative to its Call event is inconsistent between `import()` and `require()`

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/calls/mod.rs:14-55`

`record_call_expr` lowers a module call twice: for `Callee::Import` (dynamic
`import(...)`) the `else` arm emits `FactPayload::Import` *before*
`FactPayload::Call` (lines 27-38), while for a recognized `require(...)`
(callee is an `Expr::Ident`) the main path emits the Call event inside
`record_call_like` and only afterwards emits the `Import` fact (lines 52-54).
The same conceptual event ("module request with a call site") therefore has
two different relative orderings chosen by callee syntax; `TESTING.md` treats
evidence order as part of the runtime contract, and no test pins this order,
so a consumer may silently observe both. The two paths also repeat the
`if let Some(module) = module_call` emission logic.

**Recommendation:** Unify module-call lowering so the `Import` fact and its
Call event are emitted in one canonical relative order on a single path (e.g.
emit `Import` immediately before the Call event in both cases), and assert the
order in `facts/tests/build.rs`. Guardrail: keep the two facts distinct
(`Import` carries the module string, `Call` carries the call event), keep
`observe_module_call`'s interface side effects, and do not emit `Import`
twice.

**Fix Applied:** None so far.

### Provenance lifecycle

#### [ ] READ-003 — `FactProvenanceState` and `OriginChannels` expose a duplicated, unevenly split lifecycle surface

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/provenance.rs:9-243` and `provenance.rs:245-347`

Four maps share an identical checkpoint/restore/rollback/commit/snapshot/
retain-common lifecycle — `origins.instances`, `origins.classes`,
`instance_callables`, `static_string_origins` — but only two are wrapped in
`OriginChannels`; the outer `FactProvenanceState` repeats the same three-line
sequence over the other two maps in every orchestration method
(`restore_branch_entry`, `restore_instance_alternative`,
`finish_control_region`, `snapshot_instances`, `restore_instance_snapshot`,
`retain_common_instance`, `finish_branch_with_else`,
`finish_branch_without_else`, `replace_targets`). The split is arbitrary from
the caller's view: every operation must be applied to whichever subset each
method remembers, so adding a fifth provenance channel or changing the
lifecycle touches a dozen methods. `OriginChannels` is only ever used from
`provenance.rs`.

**Recommendation:** Move `instance_callables` and `static_string_origins`
into `OriginChannels` so all four maps are owned together, with each lifecycle
operation expressed once there using the correct per-map semantics; the outer
`FactProvenanceState` becomes a thin coordinator with domain vocabulary
(`finish_branch_with_else`, `replace_targets`) instead of a duplicated
op-per-map repeater. Do not introduce a generic "apply one operation to all
four" collection: the lifecycle operations are intentionally asymmetric per
map (e.g. `finish_control_region` commits instance callables and static-string
origins but rolls back class origins), so such an abstraction would only
relocate the asymmetry. Guardrails: preserve the branch-intersection semantics
exactly, especially the asymmetry in `finish_control_region` (instance origins
flow out of a region, class origins roll back) and the snapshot-vs-checkpoint
distinction that `control.rs` relies on; keep budget charging per map
unchanged.

**Fix Applied:** None so far.

### Capabilities

#### [ ] READ-004 — `DerivedPhaseCapabilities` has three always-covariant fields that are never set independently

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/mod.rs:39-86`

`DerivedPhaseCapabilities` stores three `DerivedPhaseAvailability` fields
(`export_origins`, `fact_index`, `effects`), but the only two transitions are
`enabled()` (all three `Enabled`) and `disable_derived_phases()` (all three
`DisabledByIncompleteAnalysis`); no code constructs or mutates a single field.
`semantic/tests.rs:75-77` even asserts all three are disabled together. The
per-phase granularity is dead structure that makes the API look finer than it
is and invites a reader to believe phases can be disabled selectively.

**Recommendation:** Collapse the three fields into one
`DerivedPhaseAvailability` (the accessor methods can remain as three names
over one value, or the consumers can read the single flag directly), and keep
the all-or-nothing disable semantics asserted in `semantic/tests.rs`.
Guardrail: if a genuinely independent per-phase disable is added later, reintroduce
granularity then; do not silently enable one phase after another was disabled.

**Fix Applied:** None so far.

### Wrapper lowering

#### [ ] READ-005 — `.call` / `.apply` unwrap arms are near-identical

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/facts/calls/wrapper.rs:16-55`

The `"call"` arm (lines 17-34) and `"apply"` arm (lines 35-52) of
`try_emit_callable_wrapper_common` repeat the same tail: `resolve_target_chain`
on `member.obj`, `effective_callee_expr`, `resolve_call_callee`,
`without_this_prefix`, `name_path`, `CallUnwrap { chain_path, effective_args }`,
and `emit_call`. Only the effective-argument extraction differs (`args[1..]`
vs `try_unwrap_apply_args`). Any change to unwrap emission (a new field on
`CallUnwrap`, an error guard) must be made twice.

**Recommendation:** Extract a private helper that takes the member, span,
raw args, and already-computed `Vec<CallArgInfo>` and performs the shared
chain-resolution + `emit_call` tail; keep the two arms only for argument
extraction. Guardrail: preserve the distinct failure behavior — `call` needs
at least one argument and `apply` needs at least two — and keep the existing
order of `emit_call` relative to other facts.

**Fix Applied:** None so far.

### Stream API

#### [ ] READ-006 — `register_function_parameters` panics while the symmetric read path is fail-closed

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:175-186` and `stream.rs:294-305`

The read accessor `function_parameters` converts `FunctionId` to `usize` with
a `let-else` over `usize::try_from(...)` and returns `None` on failure
(fail-closed), while the write path `register_function_parameters` uses
`usize::try_from(id.raw()).expect("FunctionId fits in usize")` (stream.rs:300).
The panic is unreachable on supported targets today (`FunctionId::raw()`
returns `u32`), but the two symmetric
paths disagree on how an untrusted identity is handled, which contradicts the
module's own invariant ("keep unsupported input distinct") and AGENTS.md's
"Do not panic" rule.

**Recommendation:** Remove the panicking conversion. Because
`FunctionId::raw()` returns `u32` and `usize` is at least 32 bits on every
supported target, the conversion is provably total; replace the
`.expect("FunctionId fits in usize")` in `register_function_parameters` with
the infallible conversion (e.g. `id.raw() as usize`) and note the invariant in
the method's doc comment, leaving the read path's fail-closed `let-else`
fallback untouched. Guardrail: registration must still fail closed (never
partially register a function with a missing slot) and the
program-level `FunctionId::new(0)` slot behavior must be preserved.

**Fix Applied:** None so far.

#### [ ] READ-007 — `FactStream` invalidity is tracked by two overlapping signals

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:118-132`, `stream.rs:242-263`, `stream.rs:49-68`

`FactStream` maintains both a `valid: bool` flag and a typed
`FactStreamIssueSet`. In production, the only path that sets `valid = false`
is `append`, and it always also inserts `BudgetExhausted` (lines 250-259); the
consequence is that `is_valid()`/`is_structurally_valid()` are false exactly
when `budget_exhausted()` is already true, making `semantic/mod.rs:230`'s
`!stream.is_structurally_valid() && !stream.name_exhausted()` branch
unreachable in production. Two near-identical invalidity signals obscure which
condition actually failed and force callers to consult several predicates to
understand a stream.

**Recommendation:** Stop conflating the two signals in `append`: exceeding
`max_facts` or overflowing the dense-ID space is a bounded-construction
outcome, so those paths should `mark_budget_exhausted()` without also setting
`valid = false`. Keep `valid` exclusively as the structural-corruption latch
(dense-ID/sequence violations), so `is_structurally_valid()` and
`budget_exhausted()` become disjoint; then delete the now-unreachable
`!stream.is_structurally_valid() && !stream.name_exhausted()` branch in
`check_facts_budget` (semantic/mod.rs:230) and update the `valid` field doc at
stream.rs:118, which currently lists budget violations. Guardrails: keep every
exhaustion reason (`name`, `path`, `budget`, `invalid_parser_span`) distinct
and fail-closed; a budget-exhausted stream must still report `!is_valid()` so
`is_projectable` (facts/mod.rs:417) and matcher gating (matching/mod.rs:278)
are unaffected; preserve the diagnostic boundary that retains invalid streams
for reporting.

**Fix Applied:** None so far.

### Interface / exports

#### [ ] READ-008 — `record_pattern_locals` returns a `BTreeSet` its only caller discards, and exported var patterns are recollected

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/facts/interface/mod.rs:36-53`, `glass-lint-core/src/analysis/facts/mod.rs:241-243`, `glass-lint-core/src/analysis/facts/interface/exports.rs:41-59`

`ModuleInterfaceBuilder::record_pattern_locals` collects a
`BTreeSet<SmolStr>` of binding names, records them, and returns the set — but
its only caller `FactBuilder::record_pattern_locals` (facts/mod.rs:241) drops
the return value, so the collection is built only to be thrown away.
Separately, for an `export const`/`export let` declaration,
`record_export_decl` (exports.rs:43) recollects the same declarator's pattern
locals via `collect_pattern_locals` on the same AST nodes that
`visit_var_declarator` will also walk through `record_pattern_locals` (the
export pass runs before the visitor descends), i.e. redundant recomputation of
the same name set.

**Recommendation:** Change `record_pattern_locals` to return `()` — the
`BTreeSet<SmolStr>` it builds is discarded by its only caller
(`FactBuilder::record_pattern_locals`, facts/mod.rs:241). For the export
pass's Var arm, keep the independent `collect_pattern_locals` but document the
ordering dependency: `visit_export_decl` (visitor.rs:416) runs
`record_export_decl` before descending into the declarator, so the export pass
must collect the names itself and the visitor's later `record_pattern_locals`
re-collects the same set for the interface's local table. Guardrails:
`add_local` must remain idempotent, and the export pass's
`function_id_for_expr`/`static_string_value` lookups must keep running for
exported declarations.

**Fix Applied:** None so far.

### Callee/origin surface

#### [ ] READ-009 — The instance-origin surface mixes a one-call forwarding alias with a raw `(SmolStr, SmolStr)` origin tuple

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/facts/calls/callee.rs:198-203`, `callee.rs:205-231`, `callee.rs:267-272`, `glass-lint-core/src/analysis/facts/provenance.rs:7`, `instance.rs:24-26`

`instance_origin_for_constructor` (callee.rs:198-203) is a one-line alias that
merely forwards to `constructor_origin_for_expr` and is used at a single call
site (construction.rs:24-28); its name suggests it returns an instance origin
when it returns a class/constructor origin, adding vocabulary without adding
meaning. Relatedly, `type Origin = (SmolStr, SmolStr)` (provenance.rs:7) is a
raw tuple used throughout for (module, export) identity, so `instance_origin`
and `class_origin` in `TargetProvenance` (provenance.rs:39-44) are the same
type and can be silently swapped, and `InstanceCallable::class_identity`
(instance.rs:24-26) returns the same untyped pair.

**Recommendation:** Delete the `instance_origin_for_constructor` alias and
have construction.rs call `constructor_origin_for_expr` directly (or rename
the underlying method to state what it actually returns). Leave the `Origin`
alias in place: promoting it to a newtype is a cross-module conversion tracked
in Systemic Themes (`provenance`, `instance`, and `model/fact`), and fixing it
piecemeal here would churn unrelated modules without addressing the
swap-risk. Guardrails: keep `(module, export)` semantics distinct from the
`member` `SymbolPath`, preserve `InstanceCallable`'s equality behavior, and
keep the fact payload shapes unchanged.

**Fix Applied:** None so far.

## Systemic Themes

- **Single-walk fact construction is coherent.** `FactBuilder` spreads its
  inherent `impl` blocks across ten files (arguments, assignments, calls,
  construction, control, functions, pattern, …) with one semantic role per
  file; this matches the architecture's "one canonical walk, matcher-index and
  effect derivation from the frozen stream" invariant and should be retained.
- **The `(SmolStr, SmolStr)` origin tuple recurs throughout the chunk** (the
  `Origin` alias, `TargetProvenance`, `BranchProvenance`,
  `InstanceCallable::class_identity`, provenance channel types). It is a
  candidate newtype, but only READ-009's swap-risk aspect is reported here;
  a full audit-level conversion should be coordinated across `provenance`,
  `instance`, and `model/fact`.
- **The 36 visit methods in `visitor.rs` each open with the identical
  `if self.resolver.budget.exhausted() { return; }` guard.** It is a
  deliberate fail-closed repetition (children and resolver work must not run
  after exhaustion), so it is defensible, but any future macro or guard helper
  should be weighed for clarity against the current explicit pattern.
- **The `ModuleInterfaceBuilder` is a legitimate boundary adapter**, not a
  pure facade: it keeps SWC AST types (`ImportDecl`, `Decl`, `Pat`) out of the
  model `ModuleInterface` and adds the `record_*` vocabulary used during the
  walk. Only the discarded-return issue in READ-008 is reported.
- **Fail-closed design is consistent and strong** in this chunk: the typed
  `FactStreamIssueSet`, the `FactStreamToken` construction authority, checked
  `FactId::index`, budget-gated emission, and `is_projectable` all preserve
  the "unsupported/exhausted is not success-empty" invariant. Do not regress
  this in any simplification.

## Open Questions

- Whether per-phase `DerivedPhaseCapabilities` granularity is planned (e.g.
  disabling only `effects` while keeping `fact_index`) is a roadmap question
  the code cannot answer. Today `enabled()` is the only constructor and
  `disable_derived_phases()` (semantic/mod.rs:281) the only transition, so
  READ-004's all-or-nothing assumption holds in the current code.
- Resolved: no consumer relies on the relative order of `Import` vs `Call`
  facts. The matcher index records each into separate literal/call indexes
  (matching/build.rs:108), and flow/projection consume module requests through
  the interface model rather than the fact stream, so the order difference in
  READ-002 is purely an evidence-order consistency concern.
- Resolved: `visit_export_decl` (visitor.rs:416-422) calls `record_export_decl`
  before descending into the declarator (`export.decl.visit_with(self)`), so
  the export pass runs first and must collect the names itself; the visitor's
  later `record_pattern_locals` re-collects the same set for the interface's
  local table. The double collection is a consequence of visit order and
  should be documented, not restructured (see READ-008).

## Coverage

Audited files (read in full):

- `glass-lint-core/src/analysis/mod.rs` — `DerivedPhaseAvailability`,
  `DerivedPhaseCapabilities`, module wiring.
- `glass-lint-core/src/analysis/facts/mod.rs` — `FactBuilder`, `build`,
  `BuiltFacts`, `SemanticFacts`, budget/name/path guards, static-import and
  export recording.
- `glass-lint-core/src/analysis/facts/stream.rs` — `FactStream<Phase>`,
  `FactStreamToken`, `FactStreamIssueSet`, freeze, parameter bindings.
- `glass-lint-core/src/analysis/facts/origin_map.rs` — `OriginMap`,
  `OriginSnapshot`, `OriginCheckpoint`, change-log rollback.
- `glass-lint-core/src/analysis/facts/provenance.rs` — `FactProvenanceState`,
  `OriginChannels`, `TargetProvenance`, checkpoints/branch joins.
- `glass-lint-core/src/analysis/facts/arguments.rs`, `assignments.rs`,
  `call_results.rs`, `construction.rs`, `control.rs`, `functions.rs`,
  `instance.rs`, `pattern.rs`, `state.rs`.
- `glass-lint-core/src/analysis/facts/calls/{mod,callee,wrapper}.rs`.
- `glass-lint-core/src/analysis/facts/interface/{mod,exports,commonjs}.rs`.
- `glass-lint-core/src/analysis/facts/visitor.rs` — the SWC `Visit` impl.
- Tests: `facts/stream_tests.rs`, `facts/tests/{build,control,stream,mod}.rs`.

Contract traced across the chunk boundary (evidence for the chunk's API
surface and invariants):

- `facts::build` -> `BuiltFacts` -> freeze path in
  `analysis/semantic/mod.rs:329, 366-401` and `Resolver::freeze_into`.
- `SemanticFacts::from_analysis` / `is_projectable` / `stream` / `matcher_index`
  consumed in `analysis/semantic/mod.rs:393`,
  `analysis/project/projection.rs:155, 266-285`, `project/identities.rs:34`,
  `project/model.rs:362-386`.
- `DerivedPhaseCapabilities` consumption in `analysis/local.rs:361-399` and
  `analysis/semantic/mod.rs:348`.
- `OccurrenceIndexes::from_stream` availability use in `analysis/matching/mod.rs`.
- `check_facts_budget` reason mapping in `analysis/semantic/mod.rs:207-238`.

`git status --short` confirmed only this audit file is untracked; no source,
test, config, or documentation file was modified.
