# Codebase Readability Audit — glass-lint-core Chunk 8: Flow effects and planning

## Summary

Chunk 8 owns `analysis::flow::effect` (per-function effect records plus the
`CallEffectRef`/`CallShape` call-shape view over the frozen fact stream),
`analysis::flow::matcher` (shared value predicates), and
`analysis::flow::planning` (pre-bound source/sink/requirement plans consumed by
the local and cross-module flow phases). The chunk's contract with sibling
chunks is strong: effects are built once from the canonical fact tape,
invalid summaries stay fail-closed across module boundaries, and the planning
boundary pre-resolves symbol paths so projection never repeats
`NamePath::from_symbol_path`. The builders (`FunctionEffectsBuilder`) and the
bounded `FlowCompletion` bit-set are well owned.

The main readability debt is the `CallShape` view, which exposes the same
"call target" concept through four overlapping accessors (`chain`, `rooted`,
`global_name`, `chain_owned`) whose sources and fallback precedence differ, and
which the local projector and the cross-flow phase consume through different
members with asymmetric fallback behavior. Secondary issues are the
re-construction of the stateless `FlowMatchView` pair at every match site, a
single-use-only constructors on `BoundLifecycleCallTarget`, a duplicated
source-index build between `BoundFlowPlan::new` and `FlowSources::collect_candidates`,
and the availability/completion coupling in `FunctionEffects` that makes a
disabled phase look like a successful empty result unless callers remember a
second flag.

No source, test, config, or documentation file was modified.

## Findings

### [effect/domain.rs — CallShape call-target view]

#### [x] READ-001 — CallShape fragments one "call target" concept across four accessors with inconsistent precedence

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/effect/domain.rs:88-97,157-178,181-222`

`CallShape` is a borrowed projection of one `CallEvent`, but the single
notion "which path did this call resolve to" is spread over four members with
independent sources: `chain` (built from `unwrap.chain_path`, then
`rooted_chain`, then `syntactic_path`), `rooted` (only whether
`rooted_chain()` was present, so the flag can describe a different source than
the chain actually returned), `global_name` (from `call_provenance`), and
`callee_name` (used only by the `chain_owned` fallback). The precedence rule is
silently duplicated by each consumer instead of being owned by the view:
`BoundTargetIndex::candidates_for_call` (`planning.rs:115-124`) re-derives the
"global first, then rooted chain" policy from `global_name`+`rooted`+`chain`,
the local projector uses `chain_owned` (`projector/driver.rs:295`), and the
cross phase uses the borrowed `chain` (`cross/propagation.rs:135`). The two
flow phases therefore resolve the *same call event* through different members:
`chain_owned` adds the `callee_name`→`NamePath` translation as a fallback
`(domain.rs:186-197)` while the cross phase's `chain()` lacks it, so an alias
call can carry a chain in the local projector and `None` in the cross
requirement matcher for the same fact. `rooted` is derived independently of the
chain source, so for a wrapper call whose chain comes from `unwrap.chain_path`
the flag may disagree with the path used for lookup, which the caller of
`candidates_for_call` must reconcile by hand.

**Recommendation:** Make `CallShape` own the member-path resolution in one
accessor whose precedence (wrapper `chain_path` → `rooted_chain` →
`syntactic_path` → `callee_name` fallback) is defined exactly once, inside the
view, and keep `rooted()` and `global_name()` as distinct semantic flags.
Route both the local `transfer_call` and the cross `apply_receiver` requirement
matching through that accessor so alias calls resolve identically in both
phases. Guardrail: do not collapse the distinct `rooted` (proven-root identity)
and `syntactic` (pure syntax) notions into one flag, and keep the existing
precedence (wrapper chain → rooted → syntactic → callee-name fallback) exactly
once, inside the view.

**Fix Applied:** `CallShape::chain` now owns the single member-path resolution with precedence wrapper `chain_path` → `rooted_chain` → `syntactic_path` → callee-name fallback (fallback computed at shape construction, fail-closed to `None` on unresolvable callee). Both the local `transfer_call` (driver.rs) and the cross `apply_receiver` (propagation.rs) route requirement matching through `shape.chain()`, so alias calls resolve identically in both phases. `rooted()` and `global_name()` remain distinct semantic flags; the rooted-gated member candidacy in `candidates_for_call` is unchanged. Tests updated to the single accessor.

#### [x] READ-002 — `chain_owned` adds a second NamePath-resolution path that duplicates name-table lookup

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/flow/effect/domain.rs:186-197`

`CallShape::chain_owned` re-implements a borrowed-vs-owned name resolution
that reproduces machinery already owned by the name table: it converts
`callee_name` to `NamePath` via `names.lookup_path(&SymbolPath::from(name))`
and wraps it in `Cow` so the borrowed chain path and the owned fallback share
one return type. The only production caller
(`projector/driver.rs:295`) immediately unwraps the `Cow` to a `&NamePath` for
`record_configuration`, so the owned variant adds a second translation path
(`SymbolPath` → `NamePath`) that exists nowhere else in the chunk and forces
the local phase to treat "chain from resolution" and "chain from callee name"
as interchangeable when the two have different confidence. `Cow` here is a
two-variant surface for what is a single "member chain for requirement
matching" value.

**Recommendation:** Fold the callee-name fallback into the canonical accessor
from READ-001 so one owned-or-borrowed resolution path exists and the
`Cow`/`SymbolPath` bridge disappears. Guardrail: keep the fallback resolution
bounded and deterministic, and keep the fail-closed behavior where an
unresolvable call yields no configuration requirements rather than an invented
path.

**Fix Applied:** Folded into the READ-001 fix. `chain_owned` is deleted; the single canonical accessor `CallShape::chain` owns the one resolution path with the callee-name fallback computed once at shape construction (bounded, deterministic), and both flow phases use it. The `Cow`/`SymbolPath` bridge disappears: no owned-vs-borrowed `Cow` surface remains, and an unresolvable call yields `None` so no configuration requirements are produced.

### [planning.rs — FlowMatchView and planning construction]

#### [x] READ-003 — `FlowMatchView` is a stateless pair re-constructed at every match site

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:35-68`; callers `projector/transfer.rs:61`, `projector/evidence.rs:41`, `cross/propagation.rs:136`, `cross/sources.rs:234`

`FlowMatchView` is a two-reference aggregate (`&NameTable`, `&ValueTable`)
with methods that are pure forwarding into `ArgumentMatcher::matches`. It adds
no invariant, no vocabulary change, and no storage, yet every consumer
reconstructs it identically in per-fact hot loops
(`FlowMatchView::new(self.inputs.names, self.inputs.stream.values())` /
`new(names, stream.values())` at four verified sites). The names/values pair
is already co-located on
`FactStream<Frozen>`, so the view is a convenience tuple whose construction is
repeated instead of hoisted, and any future consumer must learn to build it
again before it can call `BoundSource::matches_call` or the requirement
matchers.

**Recommendation:** Build the view once per phase boundary (for example in
`ProjectionInputs::new` for the local projector, once per module in the cross
source collector, and once per `UsageProjector`) and reuse that `&FlowMatchView`
for `matches_call`, `matching_member_requirement_indices`, and the record-sinks
paths, deleting the four construction sites; the consuming methods already
accept `&FlowMatchView`, so only the construction moves. Guardrail: keep the
view borrowed and immutable; do not store a `ValueTable` inside a plan that
outlives the module's value arena.

**Fix Applied:** The four construction sites are deleted. The view is now built once per phase boundary: in `ProjectionInputs::new` (local projector, used by `match_source` and `record_configuration`), once per module in the cross source collector (`collect_candidates`), and once per `UsageProjector` (cross `apply_receiver`). `UsageProjector` also hoists the per-module `FactStream` borrow that the three apply paths re-fetched. The view stays borrowed and immutable; no `ValueTable` is stored in any plan.

#### [x] READ-004 — `BoundLifecycleCallTarget::member`/`global` are single-call-site constructors and force a redundant clone

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:70-91,106-124`

`BoundLifecycleCallTarget::member` and `global` are each used exactly once, both
inside `BoundTargetIndex::candidates_for_call` (`planning.rs:117` and
`:122`), while every real binding flows through `from_lifecycle`
(`planning.rs:77-82`). The `global` helper also forces `name.clone()` of a
`SmolStr` inside the `and_then` closure because the index keys are owned while
the shape borrows, so the candidate lookup allocates a key per call. The two
one-line constructors add no semantic value over a direct match against the
enum variants.

**Recommendation:** Delete `member`/`global` and construct the candidate key
inline in `candidates_for_call` (or change the lookup to accept a borrowed
`&str`/`&NamePath` probe so global candidates avoid cloning). Guardrail: keep
the global-before-rooted precedence and the `BTreeMap` key ordering, which the
deterministic candidate ordering relies on.

**Fix Applied:** `BoundLifecycleCallTarget::member`/`global` are deleted. `BoundTargetIndex` now owns two `BTreeMap`s (`globals` keyed by `SmolStr`, `members` keyed by `NamePath`), so `candidates_for_call` looks up borrowed `&SmolStr`/`&NamePath` probes with no per-call clone, while preserving the global-before-rooted precedence and per-map `BTreeMap` key ordering (normalize sorts and dedups both maps).

#### [x] READ-005 — Source-index binding is duplicated between `BoundFlowPlan::new` and `FlowSources::collect_candidates`

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:155-170,302-306`; `cross/sources.rs:219-223`

`build_source_index` (`planning.rs:155-170`) already owns the shared sequence —
resolve `LifecycleCallTarget` to `BoundLifecycleCallTarget`, insert, `normalize()`
— and both `BoundFlowPlan::new` (`planning.rs:302-306`) and
`FlowSources::collect_candidates` (`cross/sources.rs:219-223`) call it. The
residual duplication is the value closure
`BoundSource::new(id, source.argument_constraints().clone())`, passed verbatim
at both sites, so the `BoundSource` construction rule still lives in two
callers and can silently diverge if only one is updated when the bindings or
normalization change.

**Recommendation:** Hoist the duplicated `BoundSource` value closure into one
planning-boundary helper (for example a thin wrapper over `build_source_index`
that supplies the `BoundSource::new(id, source.argument_constraints().clone())`
binding) so both `BoundFlowPlan::new` and the cross source collector call it and
the target-binding/normalization rule has one owner. Guardrail: the cross phase
deliberately re-indexes per module and per run with its own budget; keep that
phase-local rebuild but make it call the shared helper instead of passing its
own copy of the closure.

**Fix Applied:** The generic `build_source_index` is now the concrete `build_bound_source_index`, which owns the one `BoundSource::new(id, source.argument_constraints().clone())` binding alongside target binding and normalization. Both `BoundFlowPlan::new` and the cross `collect_candidates` call the same helper; the cross phase keeps its per-module/per-run re-index.

### [effect/mod.rs — FunctionEffects availability/completion coupling]

#### [x] READ-006 — A disabled `FunctionEffects` reports a complete completion, making disabled look like successful-empty

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:282-298,439-458`

When effects are disabled (`DerivedPhaseAvailability::DisabledByIncompleteAnalysis`),
`FunctionEffectsBuilder::finish` returns `FunctionEffects` with an empty table,
`completion: FlowCompletion::default()` (complete), and only
`availability = Disabled…` distinguishing it from a genuinely empty, successful
collection. Consumers must remember to check `is_available()` *before*
`completion()` or an incomplete run, and the contract is verified in
`project/projection.rs:174-179,281`; any future caller that inspects only
`completion().is_complete()` would treat a disabled phase as a clean empty
result. This is exactly the "unsupported/incomplete work must stay distinct
from a successful empty" situation: the distinction currently lives in a
second boolean rather than in the completion state itself.

**Recommendation:** Make the disabled state observable through the same
surface used for bounded-exhaustion decisions — for example have `completion()`
report an incomplete reason (or a dedicated reason) when the phase was
disabled, or expose a single tri-state status accessor, and keep
`is_available()` as the phase-gating flag where it is cheap. Guardrail: do not
lose the existing fail-closed behavior where disabled effects produce no
qualified propagation; the local projector must still short-circuit on
availability without allocating flow state.

**Fix Applied:** `finish()` now returns `completion = FlowCompletion::incomplete(FlowCompletionReason::PhaseDisabled)` when the phase was disabled, so a consumer inspecting only `completion().is_complete()` sees an incomplete (distinct from successful-empty) result. `is_available()` remains the phase-gating flag and the local projector still short-circuits on it without allocating flow state, preserving fail-closed behavior. Added a focused test asserting disabled effects report an incomplete completion with no effect rows.

#### [ ] READ-007 — `value_roots` and `parameter_index` are parallel maps whose consistency is caller-maintained

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:35-36,59-79,131-133,215-237`

`FunctionEffect` keeps the value/root relation across two `HashMap`s:
`value_roots: HashMap<ValueId, ValueId>` and
`parameter_index: HashMap<ValueId, ParameterRef>`, seeded together in
`with_parameters` (`mod.rs:59-79`). `parameter_index` is never written again;
`value_roots` is the only map mutated afterward, by `record_copy`/`copy_root`
(`:219-229`) and `record_reference` (`:239-249`) — `record_return` (`:251-279`)
mutates neither. The coupling is that parameter lookup walks `value_roots` to a
root and then asks `parameter_index` (`parameter_for`, `:231-237`), and the
`unwrap_or(value)` fallback means a root entry erased by `copy_root` (for
example a parameter's self-root) is silently treated as the value itself, so
correctness depends on convention rather than on one owned invariant. The same
"follow the root, then ask if it is a parameter" step is re-derived in
`parameter_for` and in `copy_root`'s root resolution.

**Recommendation:** Keep the two maps (they answer different queries —
`value_root` feeds the cross phase, `parameter_for` feeds the record paths) but
own the coupling in one place: extract the shared root-following step into one
private `root_of` helper used by both `parameter_for` and `copy_root`, and
preserve the parameter self-root invariant when `copy_root` would erase it, so
the copy/reference/return record paths cannot silently diverge. Guardrail:
preserve the fail-closed rules — UNKNOWN sources erase non-parameter roots
(`:223-224`), returning an unrooted local value marks the effect invalid
(`:258-264`), and an invalid summary must not propagate qualified flow.

**Fix Applied:** None so far.

## Systemic Themes

- **Call-target resolution is not owned by one type.** The `CallShape` view and
  `BoundTargetIndex` each re-derive "global vs. rooted member vs. syntactic"
  precedence (`domain.rs:157-178`, `planning.rs:115-124`), and the local and
  cross phases consume different accessors with different fallbacks
  (`driver.rs:295` vs `propagation.rs:135`). Any change to the precedence rule
  touches three files.
- **Stateless pair-wrappers get rebuilt everywhere.** `FlowMatchView` (four
  sites) shows the same shape as the accepted `CallEffectRef` borrow-preserving
  view, but unlike `CallEffectRef` it adds no borrow or contract value, only
  repeated construction.
- **Boundary-built aggregates are rebuilt instead of shared.** `build_source_index`
  runs twice; availability vs. completion is split across two fields. Both
  would be clearer with one owning constructor/status surface.
- **Unsupported vs. empty is handled carefully but inconsistently.** The
  `invalid` effect flag, the `is_available` gate, and `FlowCompletion` each
  model a different incompleteness, and consumers must combine them in the
  right order (`projection.rs:174-179`, `summaries.rs:127-128`,
  `identities.rs:67-68`). The audit did not find a concrete certainty bug here;
  it is a documentation/encapsulation concern.

## Open Questions

- Resolved: no. Member candidacy is intentionally gated on `rooted()`: the
  precedence comment on `BoundFlowPlan::source_candidates_for_call`
  (`planning.rs:343-344`) documents "a global target first, then a rooted
  member target", and `LifecycleCallTarget::RootedMember` binds only when
  `names.lookup_path` resolves it (`planning.rs:79`). A wrapper call's
  `chain()` comes from `unwrap.chain_path`, which
  `try_emit_callable_wrapper_common` derives from the *target* object
  (`foo.bar`) of `.call`/`.apply` (`facts/calls/wrapper.rs:27-32`), so the
  chain and `rooted()` describe the same target; `rooted()` is false exactly
  when that target is not provably rooted, in which case it must not be a
  member candidate. The tests only cover the agreeing cases
  (`effect/tests.rs:38-39,81`); the disagreeing case (chain present, rooted
  false) is the intended "syntactic path without a proven root" outcome and is
  the asymmetry that READ-001 targets.
- Resolved: the per-module cross index is intentionally isolated and cannot
  share a `BoundFlowPlan` index. Name resolution is module-local: the index is
  built with each module's own name table (`cross/sources.rs:214-219`), while a
  `BoundFlowPlan` is built per flow and per module with that module's names
  (`cross/mod.rs:182-188`). Granularity also differs: `collect_candidates`
  indexes *all* flows for candidate discovery, whereas a `BoundFlowPlan`'s
  source/sink index serves one flow's requirement/sink lookup
  (`planning.rs:345-356`). This matches the READ-005 guardrail that the cross
  phase deliberately re-indexes per module and per run with its own budget.

## Coverage

- `glass-lint-core/src/analysis/flow/effect/mod.rs` — fully reviewed (462 lines).
- `glass-lint-core/src/analysis/flow/effect/domain.rs` — fully reviewed (236 lines).
- `glass-lint-core/src/analysis/flow/effect/tests.rs` — reviewed for intended semantics.
- `glass-lint-core/src/analysis/flow/matcher.rs` — reviewed; no findings (trait is the
  narrowest shared owner for `ArgumentData` and is consumed by both phases).
- `glass-lint-core/src/analysis/flow/planning.rs` — fully reviewed (414 lines).
- `glass-lint-core/src/analysis/flow/mod.rs` — reviewed; `FlowCompletion` bit-set
  design is intentional (multi-reason merge) and well owned.
- Representative consumers traced: `flow/cross/{propagation,evidence,sources}.rs`,
  `flow/summary/{summaries,sink}.rs`, `flow/projector/{driver,transfer,evidence,mod}.rs`,
  `project/{projection,identities}.rs`, `analysis/local.rs:392-394`.
- Safety checks: the only `expect` in the chunk (`planning.rs:294`,
  `SinkIndex::new`) is guarded by the compiler-enforced 64-entry sink cap
  (`api/rule/query/limits.rs:10`, `api/compiler/physical.rs:247-252`); the
  `FunctionEffects::collect` test-only limit is restricted to `#[cfg(test)]`
  call sites; no panics or discarded `Result`s found in the chunk.
