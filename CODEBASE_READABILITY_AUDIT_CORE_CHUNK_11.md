# Codebase Readability Audit

Chunk 11 — "Retained model facts and flow"
(`glass-lint-core/src/analysis/model/{mod.rs,fact.rs,flow.rs,flow/limits.rs,flow/state.rs}`)

## Summary

This chunk owns the retained provider-neutral domain model: the semantic fact
stream and its payloads (`fact.rs`), the bounded lifecycle/flow indexes and
completion policy (`flow.rs` + `flow/state.rs`), and budget-scaled resource
limits (`flow/limits.rs`). Overall the retained layer is well-factored:
`BoundedIndex`/`RequirementIndex`/`SinkIndex` and `IndexedEvidence`/`mask` put
the bounded-domain and bit-readiness arithmetic behind one private owner,
`FlowState`/`FlowStateKey` expose narrow accessors with no leaked storage,
`Building`/`Frozen` compile-check the freeze ordering, and `SemanticFact::new`
correctly requires a construction token. The problems concentrate where the
retained model meets its callers: the completion policy is encoded twice (model
readiness enums plus 1:1 compiler IR enums that are queried separately,
READ-001), a matching-side projection copies the whole `CallEvent` surface
(READ-002), the `ValueId` → object/rooted-chain argument projection is written
in two consumers with a third divergent static-string lookup (READ-003), the
model module hosts two impl blocks that reach into producer-owned `facts` types
(READ-004), the requirements-and-sinks completion conjunction is re-derived at
both sink-anchored emission sites (READ-005), and the two `#[cfg(test)]` limits
constructors duplicate each other (READ-006). Findings READ-001..READ-006
below; no fixes applied.

## Findings

### Retained model facts and flow

#### [ ] READ-001 — Completion policy is defined twice: compiler IR enums mirror the readiness enums and are queried separately

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/api/compiler/object_flow.rs:21-32,44-59`, `glass-lint-core/src/analysis/model/flow.rs:160-197`

`RequirementMode::{AllRequired, AnyRequired}` and
`CompletionMode::{Configuration, AnySink, AllSinks}`
(`object_flow.rs:21-32`) are 1:1 mirrors of the retained
`RequirementReadiness::{Any, All}` and `SinkReadiness::{Configuration, Any,
All}` (`model/flow.rs:160-171`); `CompiledObjectFlow::readiness()`
(`object_flow.rs:44-59`) reconciles them with hand-written match arms (two for
requirements, three for completion). The policy is then queried through
**both** surfaces: `FlowState::sinks_ready` treats `SinkReadiness::Any` and
`Configuration` identically (`model/flow.rs:426` returns `true` for both — for
`Any`, the first recorded sink completes the flow), so the emitted-behavior
distinction (Configuration emits at the requirement event; Any/All anchor at a
recorded sink) is re-decided by each caller via
`completion_mode() == CompletionMode::Configuration`
(`projector/evidence.rs:186-188`, `cross/propagation.rs:200`). No single
statement of policy exists: a new completion mode must be added to the
compiler enum, the model enum, the `readiness()` lowering, the `sinks_ready()`
match, and each caller-side comparison, and a new requirement mode to the
compiler enum, the model enum, and both lowerings.

**Recommendation:** Make `model::flow::{RequirementReadiness, SinkReadiness}`
the sole owner — the direction chunk 21 (READ-001) agrees on: delete
`RequirementMode`/`CompletionMode` from `object_flow.rs`, have
`from_normalized_lifecycle` build the model enums directly and store them on
`CompiledObjectFlow` (replacing the `requirement_mode`/`completion_mode` fields
and dropping the `completion_mode()` accessor in favor of a `sink_readiness()`
one), and replace the two `completion_mode() == CompletionMode::Configuration`
checks with `sink_readiness() == SinkReadiness::Configuration`. `readiness()`
remains as the pass-through that pairs each stored enum with its count.
Guardrails: preserve the `Configuration` = "no sink required; requirement-set
completion anchors on the requirement event" versus `AnySink` = "completion
anchors on the first recorded sink" distinction, kept through the existing
`Option<NormalizedLifecycleCompletion>` source (`object_flow.rs:142-143`) so an
explicit `Configuration` stays distinct from a zero-count `AnySink`; preserve
`sinks_ready`'s trivial-true arm for `Configuration`/`Any` (`model/flow.rs:426`);
and keep the flow/completion tests behavior-identical (`flow/projector/tests.rs`,
`flow/projector/tests_extended.rs`, `api/compiler/tests`).

**Fix Applied:** None so far.

#### [x] READ-002 — `CallProjection` re-copies the entire `CallEvent` accessor surface before indexing

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/matching/build.rs:21-56`, consumers at `build.rs:177-249`

`CallProjection::from_fact` (`build.rs:35-51`) declares ten fields — nine
copied verbatim from the `CallEvent` accessor each shadows (`fact.rs:281-341`):
`callee_span, callee_name, call_provenance, syntactic_path, rooted_chain,
module_member, returned_member, instance_class, unwrap` — plus `id` from the
fact, only to be read back by `record_call_fact`/`record_call_paths`/
`record_call_special_cases` (`build.rs:177-249`). It adds nothing beyond
`occurrence()` (`build.rs:53-55`, `(fact.id, callee_span)`). Any change to
`CallEvent` (a field rename or new member) must be mirrored in this local
struct with no behavior of its own, and the model already exposes every field
through public-in-analysis accessors.

**Recommendation:** Delete `CallProjection` and its `from_fact`/`occurrence`.
Have `record_call_fact` destructure `&fact.payload` into the `CallEvent`
directly, read the accessors in place, and compute the occurrence anchor once
as `Occurrence::new(fact.id, call.callee_span())` at its top, passing the
borrowed `&CallEvent` plus that occurrence to `record_call_paths` and
`record_call_special_cases`. Do not add a `CallEvent::occurrence` method:
`Occurrence` is a `matching::occurrence` type (`occurrence/storage.rs:15-22`),
so the model cannot and should not depend on `matching`. Guardrails: keep
occurrence anchors as `(fact.id, fact.callee_span())` — the callee span, not
`fact.span`, is the anchor used for span-normalized index dedup — and keep the
borrow lifetimes unchanged (`&CallEvent` comes from `fact.payload`, the
`&SemanticFact` is already in scope).

**Fix Applied:** Removed the field-copying `CallProjection` façade. Call-index
collection now borrows the original `CallEvent` and computes the canonical
`(fact.id, callee_span)` occurrence once before passing it to the path and
special-case helpers. Verified with `make fmt && make ci`.

#### [ ] READ-003 — The cached `ArgumentView` re-implements the `CallArgInfo` argument-projection in a second place

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/matcher.rs:108-139`, `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:225-246`, `glass-lint-core/src/analysis/model/fact.rs:123-145`

The `ValueId` → object/rooted-chain interpretation is encoded twice:
`ArgumentData for CallArgInfo` (`matcher.rs:108-126`) resolves
`Value::StaticObject`/`Value::RootedMember` against the `ValueTable` lazily,
one resolve per method (`matcher.rs:113-125`); `argument_with_overlay`
(`evaluator.rs:225-246`) resolves the value once and writes the same two-arm
`Value::StaticObject` → object / `Value::RootedMember` → rooted_chain selection
out again (`evaluator.rs:231-235`) to populate an `ArgumentView`. The
`static_string` channel is genuinely different per consumer and must not be
merged: `CallArgInfo::static_string` reads only the value table
(`values.static_string(self.value)`, `matcher.rs:109-111`), while
`argument_with_overlay` resolves the project overlay first — result identity,
then module identity, then the local `Value::StaticString` fallback
(`EffectiveIdentityResolver`, `evaluator.rs:100-102,136-148`). The view's
one-shot `with_static_string` / `with_static_object` / `with_rooted_chain`
builder (`fact.rs:130-145`) has no invariant to protect: `argument_with_overlay`
is its only producer and runs each setter exactly once, and the fields are
already `pub(in crate::analysis)` (`fact.rs:125-127`).

**Recommendation:** Extract the shared two-channel selection as one model-owned
helper — e.g. `Value::object_and_chain(&self) -> (Option<&StaticObject>,
Option<&NamePath>)` in `model/value.rs`, the type that owns the `Value`
variants — and have both `ArgumentData for CallArgInfo` (`matcher.rs:113-125`)
and `argument_with_overlay` (`evaluator.rs:231-235`) derive object/rooted_chain
from it. Do not put the helper in `flow/matcher.rs`: `matching` currently has
no dependency on `flow`, and routing `evaluator.rs` through it would add that
edge. Replace the three `with_*` builders with a single `ArgumentView`
constructor (or direct field assignment, since the fields are already
crate-internal), deleting the one-shot builder chain. Guardrails: keep the
`Value::StaticObject`/`Value::RootedMember` mutual exclusion identical in both
consumers; keep each consumer's static-string resolution exactly as-is —
value-table-only for the flow matcher, identity-overlay-then-value-table for
the evaluator — because merging them would change which strings match; and
keep the cache per-argument-per-group (`constraints_match` builds a fresh
`ArgumentView` per group, `evaluator.rs:254-267`), with no shared mutation
between calls.

**Fix Applied:** None so far.

#### [ ] READ-004 — The model module defines behavior on two facts-layer types, inverting its boundary

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/model/fact.rs:4-11,353-383,454-474`, `glass-lint-core/src/analysis/facts/calls/mod.rs:90`

`model/fact.rs` imports `facts::{ResolvedCallee, stream::FactStreamToken}`
(`fact.rs:4-5`) and hosts `impl ResolvedCallee { into_call_event }`
(`fact.rs:353-383`) and `SemanticFact::new(_, _authority: FactStreamToken, ...)`
(`fact.rs:455-468`). `ResolvedCallee` is owned by `analysis::facts::calls`
(`callee.rs:14-29`), `FactStreamToken` by `analysis::facts::stream`
(`stream.rs:33-46`), and both are one-layer-up producer types; the retained
model is supposed to be the stable surface others consume. The result is an
inverted logical edge: `facts` re-exports `model::fact` types
(`facts/mod.rs:59-62`) while `model::fact` reaches back into `facts` — the only
place the retained layer imports `analysis::facts`. The
`ResolvedCallee → CallEvent` lowering (whose only caller is `emit_call` at
`facts/calls/mod.rs:90`) lives with the model file.

**Recommendation:** Move `into_call_event` to the type's owner
(`analysis/facts/calls`, next to `ResolvedCallee` in `callee.rs`). For the
token-gated constructor, either move `impl SemanticFact::new` into
`analysis/facts/stream.rs` (where the token is minted, `stream.rs:37-46`, and
consumed, `stream.rs:262`) or move `FactStreamToken` into the model beside the
`Building`/`Frozen` markers (`fact.rs:478-485`) so `model::fact` imports from
`facts` exactly nowhere. Guardrails: retain exactly one construction path for
`SemanticFact` — the stream's `append` (`stream.rs:262`); `FactStreamToken::new`
is private to the stream (`stream.rs:37-40`), so no caller-written facts exist
outside `#[cfg(test)]` — keep `into_call_event`'s contract that all names/paths
are pre-interned by the producer before it is called (`calls/mod.rs:85-97`),
and keep the `Building`→`Frozen` freeze ordering on `FactStream` unchanged
(`stream.rs:329-340`).

**Fix Applied:** None so far.

#### [x] READ-005 — Requirement and sink readiness are re-conjoined by every sink-completion caller

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/model/flow/state.rs:106-124`, `glass-lint-core/src/analysis/flow/projector/evidence.rs:171-174`, `glass-lint-core/src/analysis/flow/cross/state.rs:160-171`

`FlowState` splits completion into `is_ready` (`state.rs:106-108`, requirements
only) and `sinks_ready` (`state.rs:122-124`), and `LifecycleEvidence` mirrors
the split (`requirements_ready`/`sinks_ready`, `flow.rs:409-429`), but every
sink-anchored emission re-derives the conjunction: `state.is_ready(r) &&
state.sinks_ready(r)` (`evidence.rs:173`) and `requirements_ready(r) &&
sinks_ready(r)` on the raw evidence (`cross/state.rs:162-163`). The policy
union "the flow is complete" therefore lives only in scattered call sites
rather than on the type that owns both evidence stores.

**Recommendation:** Add one method on the type that owns both queries,
`LifecycleEvidence::complete(readiness)` in `model/flow.rs`, implemented as
`self.requirements_ready(readiness) && self.sinks_ready(readiness)`. Forward it
through `FlowState` next to `is_ready`/`sinks_ready` (`state.rs:106-124`) and
route both emission checks through it — `state.complete(readiness)` at
`evidence.rs:173` and `self.evidence.complete(readiness)` at
`cross/state.rs:162-163`. Leave `emit_if_ready` (`evidence.rs:186-188`) and the
`record_helper_sink` requirements pre-filter (`evidence.rs:157`) as
requirements-only checks; they are not the conjunction. Guardrails:
`cross/state.rs` must keep its extra `source.is_some()` gate
(`cross/state.rs:161`) distinct from the readiness conjunction, and
`SinkReadiness::Configuration` must still allow completion with zero recorded
sinks (via `sinks_ready`'s trivial-true arm, `flow.rs:426`).

**Fix Applied:** Added `complete` to `LifecycleEvidence` and forwarded it
through `FlowState`, then routed sink-anchored projector and cross-flow checks
through that owner-level conjunction. Configuration-only emission remains
requirements-only. Verified with `make fmt && make ci`.

#### [x] READ-006 — `FlowLimits::test_new` duplicates `test_with_operation_limit` with a hard-coded budget

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Testing
- **Location:** `glass-lint-core/src/analysis/model/flow/limits.rs:81-115`

The two `#[cfg(test)]` constructors are identical except that `test_new`
hard-codes `operations: usize::MAX` while `test_with_operation_limit` takes the
value (`limits.rs:88-95` vs `106-113`); six test call sites use `test_new`
(`flow/tests.rs:37`, `flow/projector/tests.rs:216`,
`flow/projector/tests_extended.rs:250,274,296,315`) and one uses the longer
form (`flow/projector/tests.rs:205`). Two near-identical construction paths for
the same private fields invite drift in the `alternatives: states.max(1)`
default.

**Recommendation:** Make `test_new` a one-line delegator —
`Self::test_with_operation_limit(objects, states, emissions, mutation,
usize::MAX)` — removing the duplicated struct literal while keeping all six
callers unchanged, and keep `test_with_operation_limit` as the single full
constructor. Guardrails: preserve the `usize::MAX` operations path (the four
limit-exhaustion tests in `tests_extended.rs` need the operation budget to not
be the first dimension exhausted) and keep the `alternatives: states.max(1)`
default identical in the remaining constructor.

**Fix Applied:** Made `FlowLimits::test_new` delegate to
`test_with_operation_limit` with `usize::MAX`, leaving the shared constructor
as the single owner of field initialization and preserving the existing
alternative-limit default and all callers.

## Systemic Themes

- **Completion policy has several shapes for one concept.** The declaration
  mode (`object_flow.rs`), the retained readiness enums and counts
  (`model/flow.rs:160-197`), the readiness queries
  (`model/flow.rs:409-429`), and the per-caller `completion_mode()`
  comparisons (`projector/evidence.rs:186-188`, `cross/propagation.rs:200`)
  all restate "when is a flow complete". This is the chunk's one genuinely
  duplicated invariant; READ-001 and READ-005 flag both ends. Everything else
  in the model resists caller re-derivation: `FlowState`/`FlowStateKey`,
  `IndexedEvidence`/`mask`, and `LifecycleRollback` keep their storage private
  and expose narrow, documented operations.
- **`BoundedIndex` is a good shared abstraction, not an accidental one.**
  It is private to `flow.rs`, centralizes the 64-key cap, the bit arithmetic,
  and the full-mask computation (`flow.rs:96-126`), and the typed
  `RequirementIndex`/`SinkIndex` are two genuinely distinct domains (source
  requirements vs terminal sinks) that cannot be confused. The `EvidenceIndex`
  trait is the single conversion point used by both readiness scans.
- **Inverted producer coupling.** `model/fact.rs` imports two producer-owned
  types (`ResolvedCallee`, `FactStreamToken`) to host two impl blocks
  (`into_call_event`, the `SemanticFact` construction token). It is the only
  place the retained layer reaches back into `analysis::facts`, and the edge
  exists for exactly those two impl blocks, not because the model needs facts
  otherwise (READ-004).
- **Caller-side projection wrappers.** `CallProjection`
  (`matching/build.rs`) re-declares the whole `CallEvent` accessor surface, and
  the `ValueId` → object/rooted-chain selection is re-derived by
  `ArgumentView`'s only producer (`matching/arguments/evaluator.rs`) instead of
  being owned once; the retained model surfaces are open enough that these
  copies exist and must be kept in sync (READ-002, READ-003). The `ArgumentView`
  cache itself is a good idea — the defect is that its channel derivation is
  hand-rolled at the call site.
- **Well-owned parts that need no change:** `SemanticFact`'s token-gated
  constructor plus dense-ID stream invariant; `Building`/`Frozen` phase
  markers (compiler-checked freeze ordering, `fact.rs:478-485`); `CallEvent`
  named constructors (`unknown`) and semantic accessors; `LifecycleRollback`
  as a captured-evidence phase marker distinct from live `EvidenceValues`;
  `FunctionTable` as a plain `IndexTable<FunctionId, T>` alias.

## Open Questions — Resolved

- **`CallEvent` triple arg representation (explicitly asked):** a genuine dual
  representation with a documented selector, not redundant fields. For an
  ordinary call, `CallEvent.args` holds the authored argument projection
  (`effective_call_args` = bound arguments plus the authored list,
  `calls/mod.rs:101-118`; stored as `args` by `into_call_event`,
  `fact.rs:379`); a wrapper call additionally stores the wrapper-projected list
  in `CallUnwrap::effective_args` (`wrapper.rs:35-53`), and
  `CallEvent::effective_args()` selects the unwrap list when present, else
  `args` (`fact.rs:337-340`) — the selector is documented at `fact.rs:344-346`.
  Both halves are read: the authored/bound list for helper-sink invocation
  matching (`call.args()` at `projector/driver.rs:256,266`) and helper-summary
  targets (`summaries.rs:314`); the effective list for configuration, sinks,
  and effects (`CallShape::effective_args()` from `call.effective_args()` at
  `effect/domain.rs:177`, used at `driver.rs:260-264`,
  `cross/sources.rs:227`, `cross/propagation.rs:122`, `transfer.rs:27`).
  Whether the authored list could be dropped from the retained event is a
  policy question (helper-sink matching would need a different argument
  source), not a readability one.
- **`FactPayload` variant overlap (`Construction` vs `Call`) (explicitly
  asked):** not redundant, and the variants do not overlap in data.
  `Construction` (`fact.rs:430-435`) is the pre-resolution `new` fact — callee
  span/name/provenance/rooted chain only, no receiver/result/args — produced
  by `emit_construction_fact` (`facts/construction.rs:78-96`) and consumed by
  the construction occurrence index (`build.rs:284-318`); `Call` carries the
  fully resolved event lowered from a resolved callee (`calls/mod.rs:90-98`).
  They are two different analysis stages sharing only the callee identification
  the construction index needs.
- **`FlowState` redundant fields (explicitly asked):** none are redundant.
  `flow` and `object_id` are the identity components and are re-derived by
  `key()` (`state.rs:59-61`) — they are the key's storage, not a copy;
  `source_event` is lifecycle state (the fact that created the state, used as
  the trace head at `evidence.rs:268-274` and range-asserted at
  `evidence.rs:211`) and is deliberately absent from `FlowStateKey`.
  `evidence` is the live evidence store; the struct is a faithful owner of its
  state.
- **`FactId::new` does not clamp to `MAX_FACTS`:** enforcement is fail-closed
  without a constructor clamp. The stream's append cap refuses to mint IDs at
  or above its limit (`append`, `stream.rs:253-260`; `with_limit` caps the
  limit at `MAX_FACTS`, `stream.rs:230`; `push` validates the dense sequence,
  `stream.rs:275-278`), and every indexed access goes through `index()` /
  `from_index`, which reject out-of-range IDs (`fact.rs:36-47`). A clamp in
  `new` would be dead code for every structurally valid stream; the
  budget-bounded/dense invariant is validated by `is_valid()` over the
  `valid`/`issues` flags (`stream.rs:136-138`). Consistent with the chunk's
  fail-closed philosophy.
- **`FlowReadiness.requirement_count`/`sink_count` could not be derived
  inside `IndexedEvidence`:** confirmed — the counts are the *declared*
  requirement/sink domain, which partial evidence cannot reconstruct:
  `ready_all` needs the declared count to build the full mask and compare it to
  the recorded mask (`flow.rs:331-343`), and `ready_any` uses it to reject
  out-of-domain indices (`flow.rs:320-329`). A partially satisfied `All` flow
  (declared 3, recorded 2) is indistinguishable from a satisfied one (declared
  2, recorded 2) from the mask alone. Passing the counts in the policy value is
  necessary; READ-001 removes the redundant compiler mirror, not the counts.
- **Caller-local tuple redundancy:** confirmed. `record_sinks` builds
  `(FlowStateKey, FlowId, SmallVec)` where the middle element re-supplies
  `key.flow()` (`evidence.rs:84-99`) and is used only to pass to
  `emit_completed_sink` (`evidence.rs:105`), which could read `key.flow()`
  instead — the key is already destructured at `evidence.rs:100-101`. This is
  a caller-local leak, not a retained-model defect, so it stays out of the
  findings; the trivial fix is to drop the middle element and call
  `emit_completed_sink(key.object(), key.flow(), ...)`.

## Coverage

Inspected (definitions plus representative callers):

- `src/analysis/model/fact.rs` — `FactId`, `ControlRegionId`, `ControlKind`,
  `ClassFactRole`, `FunctionBoundary`, `CallArgInfo`, `ArgumentView`,
  `ParameterBinding`, `CallUnwrap`, `ClassIdentity`, `CallEvent`,
  `FactPayload`, `SemanticFact`, `Building`, `Frozen`, `MAX_FACTS`, and the
  `ResolvedCallee::into_call_event` impl.
- `src/analysis/model/fact/tests.rs` — all payload/view/id tests.
- `src/analysis/model/flow.rs` — `FunctionTable`, `FlowId`,
  `EvidenceValues`, `BoundedIndex`, `RequirementIndex`, `SinkIndex`,
  `RequirementReadiness`, `SinkReadiness`, `FlowReadiness`, `EvidenceIndex`,
  `IndexedEvidence`, `LifecycleEvidence`, `LifecycleRollback`.
- `src/analysis/model/flow/limits.rs` — `FlowLimits` scaling and both
  test constructors; `flow/tests.rs` for all limits/index/state tests.
- `src/analysis/model/flow/state.rs` — `FlowState`, `FlowStateKey` and
  accessor forwarding.
- Producers: `analysis/facts/{mod,stream}.rs`, `facts/calls/{mod,callee,wrapper}.rs`,
  `facts/construction.rs`, `facts/functions.rs`, `facts/arguments.rs`.
- Consumers: `analysis/matching/{build.rs, arguments/evaluator.rs}`,
  `analysis/flow/matcher.rs`, `analysis/flow/projector/{mod,driver,evidence}.rs`,
  `analysis/flow/cross/{state,evidence,propagation}.rs`,
  `analysis/flow/effect/domain.rs`, `analysis/flow/summary/{parameter,sink}.rs`,
  `analysis/project/model.rs`.
- Compiler boundary: `api/compiler/object_flow.rs` (`RequirementMode`,
  `CompletionMode`, `readiness()`), `analysis/flow/planning.rs`
  (requirement/sink index derivation).

Only this audit file was created by this chunk; no source, test, configuration,
or other documentation was modified.
