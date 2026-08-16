# Codebase Readability Audit

## Summary

Chunk 8 — flow effects and planning (`analysis::flow::effect/mod.rs`,
`effect/domain.rs`, `flow/matcher.rs`, `flow/planning.rs`). The effect
`FunctionEffect`/`FunctionEffects`/`FunctionEffectsBuilder` trio is not
over-built: the builder genuinely owns construction state (shared budget,
stream borrow, per-value reference provenance) that the immutable
`FunctionEffects` value must not retain, and the reader-facing separation is
documented. `CallShape`/`CallEffectRef` are a genuinely useful borrow-preserving
call view that centralizes chain resolution for eight production consumers, so
the module boundary is coherent. The concrete issues are: duplicated
bound-target index construction in `planning.rs`, a `PropertyRequirementMatch`
wrapper consumed (filtered and destructured) immediately at its single call
site, a cross-module duplicate of the `CallArgInfo` argument-view conversion
that can drift, a two-step `CallEffectRef` borrow wrapper whose intermediate
`call_fact` is test-only, redundant `event`/position storage between
`EffectUse::CallArgument` and `EffectCall`, and construction/test-only
accessors that leak `pub(in crate::analysis)` visibility.

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
sort/dedupe" invariant, so a change to the precedence or fail-closed rule must
be applied twice.

**Recommendation:** Generalize the existing helper into one
`build_bound_target_index` in `planning.rs` that takes the entries to index and
a closure from `(&CompiledObjectFlow, index) -> Option<(BoundLifecycleCallTarget, T)>`;
call it from both `BoundFlowPlan::new` and `cross/sources.rs`, and have
`BoundFlowPlan::new` collect sink entries in the loop that already walks roots
(building `flows`) instead of in a parallel pass. Guardrails: keep the
global-before-rooted `candidates_for_call` precedence, the fail-closed dropping
of unresolvable targets (`from_lifecycle` → `None`), `sort_unstable`/`dedup`
normalization, and per-module construction (each module has its own `NameTable`).

**Fix Applied:** None so far.

#### [ ] READ-002 — `PropertyRequirementMatch` is constructed in one module and immediately filtered + destructured in its only caller

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:134-148`, `glass-lint-core/src/analysis/flow/planning.rs:371-394`, `glass-lint-core/src/analysis/flow/cross/propagation.rs:102-113`

`BoundFlowPlan::matching_property_requirements` (planning.rs:377-393) builds a
`Vec<PropertyRequirementMatch>` where `value_matches` is already computed
(planning.rs:387-390). The single consumer (`cross/propagation.rs:102-113`)
immediately does `.into_iter().filter(|m| m.value_matches()).map(PropertyRequirementMatch::index).collect()`
and then ignores the struct. The wrapper therefore splits one predicate (is a
matching property requirement) across two modules, storing a `bool` per entry
that is consumed only in that filter. Any reader must cross modules to see that
`value_matches` is never consulted except as a filter.

**Recommendation:** Move the `value_matches` decision inside
`matching_property_requirements` and have it return `Vec<RequirementIndex>` of
requirements whose property match *and* value predicate pass; delete
`PropertyRequirementMatch` and its two accessors, and simplify the caller to
`.collect()` on the returned indices directly. Guardrails: `value_matches` must
still require `value_is_precise && property == Some(expected) &&
matcher.matches_flow_value(static_value)`; keep returning `None` rather than a
match when the property is absent or the requirement is not a PropertyWrite, and
preserve the declaration-order indices and deterministic ordering.

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
`EffectUse::CallArgument { call_id, argument_index }` and let consumers that
need the event/item use the existing `FunctionEffect::call_argument(call_id,
argument_index)` or `calls()[call_id].event()` (already a `Vec` index). Keep
`EffectUse`'s accessor (`event()`) for the remaining variants or route it
through the call lookup. Guardrails: preserve the cross-flow iteration (usage
`event()` must stay available and still returns the matching fact id in stream
order for `usage_matches_context` and `ApplyArgument`), keep budget charging
per recorded use/call, and keep the `uses` ordering deterministic by fact
sequence.

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
test of the unknown-id failure path (effect/tests.rs:136-142). The eight
production call sites all immediately do `call_effect(x).shape()` in one
expression (cross/sources.rs:145, 223-224; cross/propagation.rs:118-119,
150-151; projector/transfer.rs:24-25; projector/driver.rs:253-254;
summary/sink.rs:208-209; project/identities.rs:37-38). Because the returned
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
AST-evaluation-time matching (evaluator.rs:248-268, `T = ArgumentView`). But the
`CallArgInfo` implementation derives `static_string` from the raw
`ValueTable::static_string(value)` (matcher.rs:109-111), while the evaluator's
`argument_with_overlay` (evaluator.rs:225-246) resolves the same shape through
`identity.static_string(argument.value, &argument.provenance, value)`
(evaluator.rs:239-241) — a provenance-aware, scope-identity-mediated lookup.
The two paths therefore encode the same `CallArgInfo` value projection twice,
and a value that the value table does not directly retain as a static string
(e.g. an inlined constant or provenance-resolved alias) can match at AST time
but not at flow time, silently diverging the two matching layers.

**Recommendation:** Consolidate the conversion into one owner so the two
matchers consume the same view. Prefer exposing a narrow helper on the value
model that both paths call (e.g. `ValueTable`-resident `static_string` /
`static_object` / `rooted_chain` extraction taking `&CallArgInfo`), with the
provenance-aware fallback resolved once, or reuse the evaluator's view builder
for both (keeping the evaluator's operation-charging inside the evaluator).
Guardrails: flow-time matching must not charge preparation/operation costs, the
`ArgumentMatcherKind` semantics (value / object keys / rooted expressions /
object property value) preserved, and any consolidation must not change which
arguments match, since that alters findings across positives/negatives; add a
cross-layer fixture that exercises an argument whose static string is only
reachable via provenance.

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
suites (effect/tests.rs:256, 279) sit in the child `tests` module and see
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
  value-shape extraction** (READ-005): flow-time matching on `CallArgInfo` and
  AST-time matching on `ArgumentView` both land in `ArgumentMatcher::matches`,
  but the conversion into the view is implemented twice and can drift.
- **Fail-closed behavior is consistent and should not regress:** unresolvable
  lifecycle targets are dropped (`from_lifecycle` → `None`), unknown calls yield
  no shape, budget exhaustion marks `FunctionEffects.invalid`, and incomplete
  analysis disables propagation. These points must be preserved by every
  refactor above.

## Open Questions

- Is the `EffectUse::CallArgument.event` duplication (READ-003) a deliberate
  hot-loop optimization to avoid the `calls().get(call_id)` index on every
  usage during cross-flow propagation (cross/propagation.rs:62-89)? If profiling
  justifies it, the redundancy should be documented on the field instead of
  deleted.
- Is the divergence between flow-time `values.static_string(...)` and
  evaluation-time `identity.static_string(...)` (READ-005) intentional — i.e.
  flow-time matching is deliberately provenance-free so it matches a strictly
  smaller set — or is it drift? Confirming this decides whether READ-005's
  consolidation should unite the two resolutions or just deduplicate the
  shape-extraction arms while keeping the narrowed string lookup.
- `FunctionEffectsBuilder.value_provenance` (mod.rs:364) is keyed by `ValueId`
  only and never reset across function boundaries, so a `Return` of a value
  referenced in another function can attribute that reference's provenance.
  Given `ReturnProjection.provenance` feeds qualified identity resolution
  (project/identities.rs:74-78), is a `(FunctionId, ValueId)` or per-effect
  provenance map needed for correctness in first-class-function cases, or is
  per-value provenance inherently deterministic?

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