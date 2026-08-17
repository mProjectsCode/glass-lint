# Codebase Readability Audit

## Summary

Chunk 8 — flow effects and planning (`analysis::flow::effect/mod.rs`,
`effect/domain.rs`, `flow/matcher.rs`, `flow/planning.rs`). The effect
`FunctionEffect`/`FunctionEffects`/`FunctionEffectsBuilder` trio is not
over-built: the builder genuinely owns construction state (shared budget,
stream borrow, per-value reference provenance) that the immutable
`FunctionEffects` value must not retain, and the reader-facing separation is
documented. `CallShape`/`CallEffectRef` are a genuinely useful borrow-preserving
call view that centralizes chain resolution for nine production consumers, so
the module boundary is coherent. The concrete issues are: duplicated
bound-target index construction in `planning.rs`, a
`PropertyRequirementMatch` wrapper whose `value_matches` flag is load-bearing
for two consumers (the one-caller framing in READ-002 is corrected below),
duplicated `CallArgInfo` object/rooted-chain shape extraction whose
static-string arms differ deliberately by phase, a two-step `CallEffectRef`
borrow wrapper whose intermediate `call_fact` is test-only, redundant
`event`/position storage between `EffectUse::CallArgument` and `EffectCall`,
and construction/test-only accessors that leak
`pub(in crate::analysis)` visibility.

6 findings: READ-001 — READ-006.

## Findings

### Planning and binding (`flow/planning.rs`)

#### [ ] READ-001 — Source and sink bound-target index construction are two copies of the same loop; only sources got a shared helper

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:150-170`, `glass-lint-core/src/analysis/flow/planning.rs:270-304`

`build_bound_source_index` (planning.rs:150-170) exists as a free function so
`cross::sources` (sources.rs:216-217) and `BoundFlowPlan::new` (planning.rs:
292-295) can build a normalized `BoundTargetIndex<BoundSource>`. The identical
sequence for sinks — iterate flows, `BoundLifecycleCallTarget::from_lifecycle`
(target, names), conditionally `index.insert(...)`, then `index.normalize()`
— is inlined in `BoundFlowPlan::new` (planning.rs:280-287 insert loop,
planning.rs:296 normalize). The two loops differ only in the target accessor
(`source.target()` / `sink.target()`) and the per-entry value constructor
(`BoundSource::new` / `BoundSink::new` plus a `SinkIndex` position). Keeping one
copy as a free function and the other inline gives the module two places to
encode the same "resolve a lifecycle endpoint once, skip unresolvable targets,
sort/dedupe" invariant, so a change to the endpoint-resolution or fail-closed
skip rule must be applied twice (the global-before-rooted precedence itself is
shared in `candidates_for_call`, planning.rs:113-122).

**Recommendation:** Generalize the existing helper into one
`build_bound_target_index` in `planning.rs` that takes the flows iterator,
`names`, and a per-flow mapping closure returning that flow's entries
(`Fn(FlowId, &CompiledObjectFlow) -> Vec<(BoundLifecycleCallTarget, T)>`). The
source closure iterates `flow.sources()` constructing `BoundSource`; the sink
closure iterates `flow.sinks().enumerate()` constructing `BoundSink` (with
`SinkIndex::new`). Call it from `BoundFlowPlan::new` for both sources and sinks
(replacing the inline sink loop at planning.rs:280-287 and the separate
`sinks.normalize()` at planning.rs:296) and from `cross/sources.rs` for
sources. Guardrails: keep the
global-before-rooted `candidates_for_call` precedence, the fail-closed dropping
of unresolvable targets (`from_lifecycle` → `None`), `sort_unstable`/`dedup`
normalization, and per-module construction (each module has its own `NameTable`).

**Fix Applied:** None so far.

#### [ ] READ-002 — `PropertyRequirementMatch` has two consumers, and `value_matches` is load-bearing beyond its cross-flow filter (the one-caller premise is corrected)

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Correctness scoping
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:134-148`, `glass-lint-core/src/analysis/flow/planning.rs:371-394`, `glass-lint-core/src/analysis/flow/cross/propagation.rs:102-113`, `glass-lint-core/src/analysis/flow/projector/driver.rs:376-383`

`matching_property_requirements` (planning.rs:377-393) builds a
`Vec<PropertyRequirementMatch>` where `value_matches` is computed once
(planning.rs:387-390). The original finding claimed a single consumer in
`cross/propagation.rs:102-113` that immediately filters on `value_matches`,
maps to `RequirementIndex`, and ignores the struct; that premise is wrong. The
function has **two** consumers: `cross/propagation.rs:104-113` projects the
matching indices, while
`projector/driver.rs:376-383` maps every entry to
`PropertyWriteUpdate { index, value_matches }`. The flag is not consumed only
as a filter: `FlowStateTable::apply_property_write`
(`projector/state/tables.rs:288-292`) clears the requirement unconditionally
and records the requirement event only when `value_matches` is true, so a
non-precise property write must stay distinguishable from a complete one for
local flow evidence (a match without a verified value is a partial, not a
completion).

**Recommendation:** No structural change is warranted. Both consumers need the
`(index, value_matches)` pair, and the predicate is already computed once
inside `matching_property_requirements`. Do not collapse the wrapper into
`Vec<RequirementIndex>` and do not fold the `value_matches` filter into the
planner, because that would make every `PropertyRequirementMatch` look precise
and change local-flow requirement certainty and cross-flow evidence
independent of the observed value. Keep `PropertyRequirementMatch` and its two
accessors as-is; the wrapper is the correct shared carrier, and the two
consumer projections express genuine consumer differences.

**Guardrails:** `value_matches` must stay defined as `value_is_precise &&
property == Some(expected) && matcher.matches_flow_value(static_value)`; keep
returning `None` rather than a match when the property is absent or the
requirement is not a PropertyWrite; preserve the declaration-order indices and
deterministic ordering; and keep both consumers' distinct projections (the
value-filtering of cross-flow and the flag-preserving `PropertyWriteUpdate`
mapping of the local projector).

**Fix Applied:** None so far.

### Flow effect summaries (`flow/effect/domain.rs`, `effect/mod.rs`)

#### [ ] READ-003 — `EffectUse::CallArgument` and `CallReceiver` re-store the `event` and argument position already owned by `EffectCall`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/effect/domain.rs:37-71`, `glass-lint-core/src/analysis/flow/effect/mod.rs:172-212`

`record_call` (mod.rs:172-212) pushes both an `EffectUse::CallArgument { call_id,
event, argument_index }` per argument and an `EffectCall { event, arguments }`,
where `call_id = EffectCallId::new(self.calls.len())` and `event = fact.id` in
both. `EffectCall.arguments` (`Vec<EffectArgument>`) already carries the index
as `EffectArgument::index` (domain.rs:18-22), so the argument position is stored
three times across the two vectors, and `call_id`+`event` together are just a
second reference to the same call fact. `EffectUse::CallReceiver` similarly
stores `event` and `receiver` even though the receiver use is derived from the
same call. The `uses` list is inherently a different ordering than the `calls`
list (event-interleaved across the function), and the cross-flow projector
rightly consumes both, but the duplicated `event`/position fields make the two
structures look like independent sources of truth.

**Recommendation:** Keep the two orderings but drop the replicated fields: give
`EffectUse::CallArgument { call_id, argument_index }` and let the cross-flow
projector derive the event via `calls()[call_id.index()].event()` (already a
`Vec` index) instead of reading it from the usage; `usage_matches_context`
(evidence.rs:35-41) already projects solely from `call_id`/`argument_index`.
Keep `EffectUse::CallReceiver` storing `event` and `receiver` unchanged: the
receiver is derived, cached data, and `record_call` can push a `CallReceiver`
even when the matching `EffectCall` was dropped on budget exhaustion
(mod.rs:191-208), so no call record exists to route through. Guardrails:
dropping `CallArgument.event` is sound only because a partial call record also
marks the effect invalid (`self.invalid = true`) and every consumer of `uses()`
prunes invalid effects first (`cross/mod.rs:100-102`,
`cross/sources.rs:141-142`, `cross/graph.rs:26-27`,
`summary/summaries.rs:127`) — that invariant must stay; the cross-flow
iteration must still obtain, per `CallArgument` use, the matching fact id in
stream order for the `apply_argument` shape lookup (propagation.rs:149-152) and
the `CallPropagation` `through` ordering (propagation.rs:246); keep budget
charging per recorded use/call; and keep the `uses` ordering deterministic by
fact sequence.

**Fix Applied:** None so far.

#### [ ] READ-004 — `CallEffectRef` is a two-step borrow wrapper whose only production method is `shape()`; `call_fact` is test-only

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/effect/domain.rs:80-84`, `glass-lint-core/src/analysis/flow/effect/domain.rs:135-184`

The public surface is `FactStream::call_effect(event) -> CallEffectRef` followed
by `ref.shape()`. `CallEffectRef` wraps only a copied `&'stream FactStream` and
a `FactId`, adds no invariant or lifecycle boundary of its own, and its
remaining method `call_fact()` (domain.rs:147-153) is exercised only by a unit
test of the unknown-id failure path (effect/tests.rs:136-142). All nine
production call sites bind `call_effect(...)` to a local `cref` and immediately
call `cref.shape()` (cross/graph.rs:30-31; cross/sources.rs:145, 223-224;
cross/propagation.rs:118-119, 150-151; projector/transfer.rs:24-25;
projector/driver.rs:253-254; summary/sink.rs:208-209; project/identities.rs:
37-38). Because the returned
view borrows the `'stream` reference copied out of the builder (not `self`),
a direct `stream.call_shape(event) -> Option<CallShape<'_>>` preserves the
borrow relationship and simplifies every consumer.

**Recommendation:** Replace `call_effect`/`CallEffectRef`/`shape()` with a
single `FactStream::call_shape(&self, event: FactId) -> Option<CallShape<'_>>`
(inherent, in `domain.rs`, owning the fail-closed chain resolution). Delete
`CallEffectRef` and keep the chain-resolution order (unwrap chain, rooted chain,
syntactic path, callee-name fallback) in `CallShape`'s constructor. Guardrails:
`None` for unknown/dense-invalid/uncall fact ids must remain (fail-closed), the
borrow must continue to allow `&mut self` projector calls after shape creation
(it does today because the view borrows the stream reference, not the
projector), and the failing-path unit test asserts `stream.call_shape(unknown)
.is_none()`.

**Fix Applied:** None so far.

### Shared argument matching (`flow/matcher.rs`)

#### [ ] READ-005 — `ArgumentData for CallArgInfo` is a second implementation of the `CallArgInfo` → argument-view conversion with a different static-string resolution

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/matcher.rs:100-126`, `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:225-246`

`ArgumentMatcher::matches<T: ArgumentData>` (matcher.rs:58-98) is shared by
flow-time argument matching (planning.rs:46-64, `T = CallArgInfo`) and
AST-evaluation-time matching (evaluator.rs:248-268, `T = ArgumentView`). The
`CallArgInfo` implementation derives `static_string` from the raw
`ValueTable::static_string(value)` (matcher.rs:109-111), while the evaluator's
`argument_with_overlay` (evaluator.rs:225-246) resolves the same shape through
`identity.static_string(argument.value, &argument.provenance, value)`
(evaluator.rs:239-241) — a provenance-aware, scope-identity-mediated lookup.
The two paths therefore encode the same `CallArgInfo` **object/rooted-chain
shape projection** twice — the extraction is literally identical
(`values.resolve(value)` matched against `StaticObject`/`RootedMember` in
matcher.rs:113-125 and evaluator.rs:229-238) — while their **static-string arms
differ deliberately**: flow-time matching is provenance-free by construction
(`FlowMatchView` carries only names+values, planning.rs:35-39/42-44; local flow
is invoked without any overlay, projection.rs:264-272), whereas evaluation-time
matching runs only after the project overlay exists (projection.rs:156-172;
evaluator.rs:136-148). So a value the value table does not directly retain as a
static string (e.g. an inlined constant or provenance-resolved alias) matches
at AST time but not at flow time; this is the intended narrower flow-time set,
not silent drift, and uniting the two silhouettes would change which arguments
flow-time matches.

**Recommendation:** Consolidate only the identical shape-extraction: expose one
narrow helper on the value model that resolves a `ValueId` to its
`(Option<&StaticObject>, Option<&NamePath>)` pair
(`values.resolve` → `StaticObject`/`RootedMember`) and call it from both the
`ArgumentData for CallArgInfo` impl (matcher.rs:113-125) and the evaluator's
`argument_with_overlay` (evaluator.rs:229-238), keeping the evaluator's
operation-charging inside the evaluator. Do **not** reuse the evaluator's view
builder for flow time and do not unite the two static-string resolutions:
flow-time matching must remain `ValueTable`-only (the identity overlay is not
built when flow runs) and evaluation-time matching keeps its
`identity.static_string(...)` overlay fallback (evaluator.rs:239-241) layered on
the shared shape. Guardrails: flow-time matching must not charge
preparation/operation costs, the `ArgumentMatcherKind` semantics (value /
object keys / rooted expressions / object property value) preserved, and any
consolidation must not change which arguments match, since that alters findings
across positives/negatives; add a cross-layer fixture that exercises an
argument whose static string is only reachable via provenance so the
intentional divergence stays pinned rather than silently converging.

**Fix Applied:** None so far.

#### [ ] READ-006 — Construction-state type and a test-only accessor leak `pub(in crate::analysis)` visibility

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:131-133`, `glass-lint-core/src/analysis/flow/effect/mod.rs:360-366`

`FunctionEffectsBuilder` (mod.rs:360-366) is declared `pub(in crate::analysis)`
yet its only construction and consumption happen in this module
(mod.rs:346-351, 453); no other module in the crate references the type. Its
purpose is documented as separating mutable construction from the immutable
`FunctionEffects`, which is legitimate, but the outer visibility exposes a
one-caller builder to the whole analysis surface. Likewise
`FunctionEffect::value_root` (mod.rs:131-133) is `pub(in crate::analysis)` but
every production use is the private `root_of` (mod.rs:137-139); the public test
suites (effect/tests.rs:256, 278, 283) sit in the child `tests` module and see
module-private items via `use super::*`. The wide visibility makes the builder
look like a public construction API and invites callers to bypass the frozen
`FunctionEffects` boundary.

**Recommendation:** Make `FunctionEffectsBuilder` private to the
`flow::effect` module and `FunctionEffect::value_root` module-private (or
`#[cfg(test)]`) — the tests compile against the module's private surface
unchanged. Guardrails: keep `FunctionEffects::collect_with_availability` as the
production entry point used by `local.rs:404` and the `#[cfg(test)]`
`collect` convenience, and keep the budget/availability plumbing on the builder
(exhaustion → `FlowCompletion::incomplete`), which must not regress to a
successful empty result.

**Fix Applied:** None so far.

## Systemic Themes

- **One-shot fact-stream derivations with separate per-module rebinding.** Both
  the local projector (`projector/mod.rs:130`) and the cross overlay
  (`cross/mod.rs:112-115`) rebuild a `BoundFlowPlan` per module from the same
  `BoundLifecycleRoot` slice because the plan is name-table-bound (each module
  has its own `NameTable`). `BoundLifecycleCallTarget` (planning.rs:71-84) and
  `BoundFlowPlan::req_members` (planning.rs:206-209) are the correct "resolve a
  symbol once per module" pattern; the only misapplication is the duplicated
  index-construction loop in READ-001.
- **`EffectCall` + `EffectUse` as two orderings of one event stream.** The
  `uses` list (event-interleaved) and `calls` list (per-call, argument bearing)
  are both consumed by cross-flow for different purposes; the redundancy is
  limited to re-stored `event`/position fields (READ-003), not the vectors
  themselves.
- **Finalized/immutable vs. constructing state.** `FunctionEffects` /
  `FunctionEffectsBuilder` and `BoundFlowPlan` (built once, queried during
  matching) follow the same immutable-after-construction shape used across the
  chunk. The boundary violations are visibility only (READ-006).
- **Two argument-matching surfaces with a shared predicate but duplicated
  object/rooted-chain shape extraction** (READ-005): flow-time matching on
  `CallArgInfo` and AST-time matching on `ArgumentView` both land in
  `ArgumentMatcher::matches`; the identical `values.resolve` →
  `StaticObject`/`RootedMember` extraction is implemented twice, while the
  static-string arms differ deliberately (flow-time is overlay-free by phase).
- **Fail-closed behavior is consistent and should not regress:** unresolvable
  lifecycle targets are dropped (`from_lifecycle` → `None`), unknown calls yield
  no shape, budget exhaustion marks the affected `FunctionEffect` invalid and
  the `FunctionEffects` completion incomplete, and incomplete analysis disables
  propagation. These points must be preserved by every refactor above.

## Open Questions (resolved)

- **Resolved — READ-003: `EffectUse::CallArgument.event` is not a deliberate
  hot-loop optimization.** No profiling annotation or comment motivates it; the
  per-usage `calls()[call_id.index()]` lookup is an `O(1)` Vec index over a
  small per-function `Vec<EffectCall>` (mod.rs:179, 191-195; `EffectCallId::index`
  at domain.rs:32-34). The stored `event` is consumed only in the cross-flow
  projector loop (propagation.rs:62-89) — as the `through` argument to
  `CallPropagation` and the shape lookup in `apply_argument`
  (propagation.rs:149-152) — so it is a saved indirection, not a hot-path
  requirement. Deleting it per READ-003 is sound because `record_call` pushes
  `CallArgument` uses before the `EffectCall` (mod.rs:180-190) and a budget
  failure on the call push leaves a dangling `call_id` (self.invalid = true,
  mod.rs:196-198); that partial effect is pruned by every consumer before
  `uses()` is read (cross/mod.rs:100-102, cross/sources.rs:141-142,
  cross/graph.rs:26-27, summary/summaries.rs:127). That pruning invariant is
  now an explicit READ-003 guardrail.
- **Resolved — READ-005: the flow-time vs evaluation-time static-string
  divergence is intentional and phase-forced, not drift.** Flow-time matching is
  provenance-free by construction: `FlowMatchView` holds only `names` + `values`
  (planning.rs:35-39, 42-44), and neither local flow (projection.rs:264-272) nor
  cross flow (projection.rs:133) receives the identity overlay, which is built
  only for the matching phase (projection.rs:156-172) and consumed only by
  `MatcherEvaluator` (evaluator.rs:157-168). The overlay itself depends on the
  effects (`call_result_identities`, identities.rs:25-48), so it cannot exist
  while flow is being built. Flow-time therefore matches the strictly smaller
  set by design, and READ-005 must deduplicate only the identical
  object/rooted-chain extraction arms (matcher.rs:113-125 vs evaluator.rs:
  229-238) while keeping the two static-string resolutions distinct.
- **Resolved — the `ValueId`-keyed `value_provenance` map (mod.rs:364) is
  deterministic and adequate; no `(FunctionId, ValueId)` map is required.**
  `ValueId`s are module-unique: the per-module `ValueTable` interns each value
  once (model/value.rs:157-196, 231-246), so function-local values never share
  an id with another function's values. Values referenced across functions are
  necessarily module-scope (globals, exports, recognized require results), and
  references to them resolve to the same `SymbolCallProvenance` at every site,
  so the last-writer-wins order (the deterministic tape order) cannot attribute
  a foreign provenance to a `Return` (writes at mod.rs:253-263, reads at
  mod.rs:284-287). The only consumer of `ReturnProjection.provenance` is
  identities.rs:74-84, for `parameter().is_none()` returns resolved against the
  module export/global table; a per-effect key would be speculative absent a
  demonstrated case of one id carrying two provenances. If such a case is later
  shown, `(FunctionId, ValueId)` keying is the prescribed fix.

## Coverage

Files reviewed for this chunk (Chunk 8, flow effects and planning):

- `glass-lint-core/src/analysis/flow/effect/mod.rs` (`FunctionEffect`,
  `FunctionEffects`, `FunctionEffectsBuilder`)
- `glass-lint-core/src/analysis/flow/effect/domain.rs` (`ParameterRef`,
  `EffectArgument`, `EffectCallId`, `EffectCall`, `EffectUse`,
  `ReturnProjection`, `CallEffectRef`, `CallShape`,
  `impl FactStream<Frozen>::call_effect`)
- `glass-lint-core/src/analysis/flow/effect/tests.rs`
- `glass-lint-core/src/analysis/flow/matcher.rs` (`ValueMatcher::matches_static` /
  `matches_flow_value`, `ArgumentMatcher::matches`, `ArgumentData`)
- `glass-lint-core/src/analysis/flow/planning.rs` (`FlowMatchView`,
  `BoundLifecycleCallTarget`, `BoundTargetIndex`, `PropertyRequirementMatch`,
  `build_bound_source_index`, `BoundLifecycleRoot`, `BoundFlowPlan`,
  `BoundSource`, `BoundSink`)
- Callers/lifecycle traced for the above types:
  `flow/{mod.rs,cross/{mod.rs,evidence.rs,graph.rs,sources.rs,propagation.rs,state.rs},
  projector/{mod.rs,driver.rs,transfer.rs,evidence.rs},
  summary/{summaries.rs,sink.rs}}`,
  `analysis/{local.rs,project/projection.rs}`, `project/identities.rs`,
  `matching/arguments/evaluator.rs`, `model/fact.rs`, `facts/{stream.rs,mod.rs}`.

Read-only audit; no source files were modified.