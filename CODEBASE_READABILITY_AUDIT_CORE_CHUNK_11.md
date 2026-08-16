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
correctly requires a construction token. The problems found concentrate where
the retained model meets its callers: the completion policy is encoded twice
(model readiness enums and 1:1 compiler IR enums that are queried separately),
a matching-side projection copies the whole `CallEvent` surface, the
value-to-argument view projection is re-implemented in two places, and the
model module defines behavior on two producer-owned types (`ResolvedCallee`,
`FactStreamToken`), inverting its boundary with `analysis::facts`. Findings
READ-001..READ-006 below; no fixes applied.

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
(`object_flow.rs:22-32`) are 1:1 mirrors of the retained
`RequirementReadiness::{Any, All}` and `SinkReadiness::{Configuration, Any,
All}` (`model/flow.rs:161-171`); `CompiledObjectFlow::readiness()`
(`object_flow.rs:44-59`) reconciles them with a four-arm manual match. The
policy is then queried through **both** surfaces: `FlowState::sinks_ready`
treats `SinkReadiness::Any` and `Configuration` identically (`model/flow.rs:426`
returns `true` for both), so the emitted-behavior distinction is re-decided by
each caller via `completion_mode() == CompletionMode::Configuration`
(`projector/evidence.rs:186-188`, `cross/propagation.rs:200`). Every new
completion/requirement mode must be added to six places (two enums, the match,
`FlowReadiness`, the readiness query, and each mode comparison) with no single
statement of policy.

**Recommendation:** Make `model::flow::{RequirementReadiness, SinkReadiness}`
the sole owner: delete `RequirementMode`/`CompletionMode` from
`object_flow.rs`, have `from_normalized_lifecycle` build the model enums
directly, store them on `CompiledObjectFlow`, and replace the two
`completion_mode() == CompletionMode::Configuration` checks with
`sink_readiness() == SinkReadiness::Configuration`. Guardrails: preserve
`Configuration` = "no sink required; requirement-set completion anchors on the
requirement event", preserve `sinks_ready`'s empty/trivial-true behavior, and
keep the existing `object_flow`/projector/completion tests (`projector/tests.rs`,
`tests_extended.rs`) behavior-identical.

**Fix Applied:** None so far.

#### [ ] READ-002 — `CallProjection` re-copies the entire `CallEvent` accessor surface before indexing

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Other
- **Location:** `glass-lint-core/src/analysis/matching/build.rs:21-56`, consumers at `build.rs:177-249`

`CallProjection::from_fact` (`build.rs:34-51`) declares ten reference fields —
`callee_span, callee_name, call_provenance, syntactic_path, rooted_chain,
module_member, returned_member, instance_class, unwrap` — each copied verbatim
from the `CallEvent` accessor it shadows (`fact.rs:281-341`), plus `id` from the
fact, only to be read back by `record_call_fact`/`record_call_paths`/
`record_call_special_cases` (`build.rs:201-249`). It adds nothing beyond
`occurrence()` (`build.rs:53-56`, `(fact.id, callee_span)`). Any change to
`CallEvent` (a field rename or new member) must be mirrored in this local
struct with no behavior of its own, and the model already exposes every field
through public-in-analysis accessors.

**Recommendation:** Delete `CallProjection`. Have the three `record_call_*`
functions take `(&CallEvent, FactId)` — or add one model method such as
`CallEvent::occurrence(id, span)` if `occurrence()` is worth retaining — and
read the accessors directly. Guardrails: keep occurrence anchors as
`(fact.id, fact.callee_span())` used for span-normalized index dedup, and keep
the borrow lifetimes (`&CallEvent` comes from `fact.payload`, `&SemanticFact`
already in scope).

**Fix Applied:** None so far.

#### [ ] READ-003 — The cached `ArgumentView` re-implements the `CallArgInfo` argument-projection in a second place

- **Severity:** Low
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/matcher.rs:108-139`, `glass-lint-core/src/analysis/matching/arguments/evaluator.rs:225-246`, `glass-lint-core/src/analysis/model/fact.rs:123-145`

The same `ValueId` → `(static_string, static_object, rooted_chain)`
interpretation is encoded twice: `ArgumentData for CallArgInfo`
(`matcher.rs:108-126`) resolves lazily against `ValueTable`, and
`argument_with_overlay` (`evaluator.rs:225-246`) resolves and caches the same
three channels in an `ArgumentView` whose precedence (`StaticObject` wins over
`RootedMember`) is written out again. The view's `with_static_string` /
`with_static_object` / `with_rooted_chain` builder (`fact.rs:130-145`) is
one-shot (each caller runs the chain exactly once) and its fields are
`pub(in crate::analysis)`, so the builder is scaffolding around a mutable
three-`Option` struct with no invariant to protect.

**Recommendation:** Keep `ArgumentView` as the per-call cache, but give it one
constructor computed from a single shared projection helper (e.g.
`fn project(call_arg, values) -> (Option<&str>, Option<&StaticObject>, Option<&NamePath>)`
in `flow/matcher.rs`) used by both the `CallArgInfo` trait impl and
`argument_with_overlay`, deleting the three `with_*` methods. Guardrails: the
object-vs-rooted precedence and the "static string only when the identity
table proves it" rule must remain identical across both consumers, and the
cache must stay per-argument-per-group (no shared mutation between
`constraints_match` calls).

**Fix Applied:** None so far.

#### [ ] READ-004 — The model module defines behavior on two facts-layer types, inverting its boundary

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/model/fact.rs:4-11,353-383,454-474`, `glass-lint-core/src/analysis/facts/calls/mod.rs:90`

`model/fact.rs` imports `facts::{ResolvedCallee, stream::FactStreamToken}` and
hosts `impl ResolvedCallee { into_call_event }` (`fact.rs:353-383`) and
`SemanticFact::new(_, _authority: FactStreamToken, ...)` (`fact.rs:455-468`).
`ResolvedCallee` is owned by `analysis::facts::calls`, `FactStreamToken` by
`analysis::facts::stream` (stream.rs:33-46), and both are one-layer-up producer
types; the retained model is supposed to be the stable surface others consume.
The result is a logical two-way edge: `facts` re-exports `model::fact` types
(`facts/mod.rs:59-62`) while `model::fact` reaches back into `facts`, and the
`ResolvedCallee → CallEvent` lowering (whose only caller is
`facts/calls/mod.rs:90`) lives with the model file.

**Recommendation:** Move `into_call_event` to the type's owner
(`analysis/facts/calls`), and keep the token-gated constructor next to the
stream (`analysis/facts/stream.rs`) or move the token to the model so
`model::fact` imports from `facts` exactly nowhere. Guardrails: retain exactly
one construction path for `SemanticFact` (no caller-written facts), keep
`into_call_event`'s contract that all names/paths are pre-interned by the
producer, and keep the `Building`→`Frozen` freeze ordering on `FactStream`
unchanged.

**Fix Applied:** None so far.

#### [ ] READ-005 — Requirement and sink readiness are re-conjoined by every sink-completion caller

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/model/flow/state.rs:106-124`, `glass-lint-core/src/analysis/flow/projector/evidence.rs:171-174`, `glass-lint-core/src/analysis/flow/cross/state.rs:160-171`

`FlowState` splits completion into `is_ready` (`state.rs:106-108`, requirements
only) and `sinks_ready` (`state.rs:122-124`), but every sink-anchored emission
re-derives the conjunction: `state.is_ready(r) && state.sinks_ready(r)`
(`evidence.rs:173`) and `requirements_ready(r) && sinks_ready(r)` on the raw
evidence (`cross/state.rs:162-163`). The policy union "the flow is complete"
therefore lives only in scattered call sites rather than on the type that owns
the evidence.

**Recommendation:** Add one method on the evidence owner — e.g.
`FlowState::is_complete(readiness)` / `LifecycleEvidence::complete(readiness)`
implemented as `requirements_ready && sinks_ready` — and route both emission
checks through it. Guardrails: `cross/state.rs` must keep its extra
`source.is_some()` gate distinct from the readiness conjunction, and
`SinkReadiness::Configuration` must still allow completion with zero recorded
sinks.

**Fix Applied:** None so far.

#### [ ] READ-006 — `FlowLimits::test_new` duplicates `test_with_operation_limit` with a hard-coded budget

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Testing
- **Location:** `glass-lint-core/src/analysis/model/flow/limits.rs:81-115`

The two `#[cfg(test)]` constructors are identical except that `test_new`
hard-codes `operations: usize::MAX` while `test_with_operation_limit` takes the
value (`limits.rs:88-95` vs `105-113`); six test call sites use `test_new`
(tests.rs:37, tests_extended.rs:250-315, tests.rs:216) and one uses the longer
form (tests.rs:205). Two near-identical construction paths for the same private
fields invite drift in the `alternatives: states.max(1)` default.

**Recommendation:** Delete `test_new` and route its six callers through
`test_with_operation_limit(..., usize::MAX)`, or have `test_new` call the
longer constructor. Guardrails: keep the ability to force `usize::MAX`
operations (budget-exhaustion tests rely on it) and keep the
`alternatives: states.max(1)` default identical in both paths.

**Fix Applied:** None so far.

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
  types to host impls (`into_call_event`, the `SemanticFact` construction
  token). It is the only place the retained layer reaches back into
  `analysis::facts`, and it is exactly the two impl blocks, not an inherent
  dependency (READ-004).
- **Call-site-side projection wrappers.** Both `CallProjection`
  (`matching/build.rs`) and `ArgumentView` (`matcher.rs`/`evaluator.rs`) are
  callers re-declaring or re-deriving what `CallEvent`/`CallArgInfo` already
  own; the retained model surfaces are open enough that these copies exist and
  must be kept in sync (READ-002, READ-003).
- **Well-owned parts that need no change:** `SemanticFact`'s token-gated
  constructor plus dense-ID stream invariant; `Building`/`Frozen` phase
  markers (compiler-checked freeze ordering, `fact.rs:478-485`); `CallEvent`
  named constructors (`unknown`) and semantic accessors; `LifecycleRollback`
  as a captured-evidence phase marker distinct from live `EvidenceValues`;
  `FunctionTable` as a plain `IndexTable<FunctionId, T>` alias.

## Open Questions

- **`CallEvent` triple arg representation (explicitly asked):** no finding.
  `CallEvent.args` stores the bound/effective list (for ordinary calls the
  authored list) while `unwrap.effective_args` stores the wrapper-projected
  list (`fact.rs:337-341`); both are read — the authored list for
  helper-sink invocation matching (`projector/driver.rs:256,266`) and the
  effective list for configuration/sinks. It is a genuine dual representation
  with a documented selector, not redundant fields; whether the authored list
  can be dropped from the retained event is a policy question, not a
  readability one.
- **`FactPayload` variant overlap (`Construction` vs `Call`) (explicitly
  asked):** not redundant. `Construction` (`fact.rs:430-435`) is the
  pre-resolution `new` fact (callee span/name/provenance/rooted chain, no
  receiver/result/args) produced by `facts/construction.rs`; `Call` carries the
  resolved event. They do not duplicate data.
- **`FlowState` redundant fields (explicitly asked):** none. `flow` and
  `object_id` are both the key components and the `key()` view
  (`state.rs:59-61`); `source_event` is life-cycle state, not identity.
- **`FactId::new` does not clamp to `MAX_FACTS`:** enforcement is by the
  stream's append cap plus consumer-side `index()`/`from_index` checks
  (`fact.rs:44-47`); the "dense, budget-bounded" invariant is validated at
  freeze/index time. Fail-closed and consistent with the chunk's philosophy;
  a constructor-level clamp was considered and judged redundant.
- **`FlowReadiness.requirement_count`/`sink_count` could not be derived
  inside `IndexedEvidence`:** the counts are the *declared* requirement/sink
  domain, which partial evidence cannot reconstruct, so passing them in the
  policy value is necessary (though READ-001 removes the redundant mirror).
- **Caller-local tuple redundancy:** `projector/evidence.rs:84-100` builds
  `(FlowStateKey, FlowId, SmallVec)` where `FlowId` re-supplies `key.flow()`;
  leak of caller-side, not a retained-model defect, so left out of the
  findings.

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

Only this audit file was created; no source, test, configuration, or other
documentation was modified (`git status` clean apart from this new file).