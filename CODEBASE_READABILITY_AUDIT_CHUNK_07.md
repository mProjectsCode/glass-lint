# Codebase Readability Audit — glass-lint-core Chunk 7: Cross-flow analysis

## Summary

Chunk 7 covers `analysis::flow` (`FlowCompletion`, `FlowCompletionReason`) and the
`analysis::flow::cross` pass (`cross/mod.rs` plus `evidence`, `graph`,
`propagation`, `sources`, `state`, `worklist`). The pass owns the qualified
call graph, source-candidate propagation, bounded context worklist, and
cross-file evidence emission, and is invoked once per linked project from
`project/projection.rs::collect_cross`.

The architecture is sound overall: bounded budgets (`Budget`, `MAX_CONTEXTS`,
`MAX_PENDING`), deterministic ordered collections, fail-closed evidence policy
(`mark_all_possible` on exhaustion, `nonmatching` keys kept separate from
witness traces), and a clear split between local flow (projector/summary,
chunk 9/10) and this cross overlay. No production panics, unwraps, or discarded
`Result`s were found in the chunk; all `unwrap_or` sites are identity
fallbacks.

The main structural issue is redundant plan construction: the cross pass
rebuilds a `BoundFlowPlan` per `(flow, module)` pair even though the local
projector already builds a per-module plan over the same roots. Secondary
issues are parallel/duplicate small types (`ContextAdmission`, `EmissionContext`,
`EvidenceKey` construction, `CallPropagation.module`, test-only `SourceBudget`)
and a repeated `value_root` identity-fallback lookup across four sites. The
chunk also shows a recurring "one borrow-packaging struct per pass" pattern
(`CrossProjectionSession` → `ContextProjection` → `UsageProjector` /
`CallPropagation` / `EmissionContext`), which is consistent but adds
indirection layers worth a deliberate second look.

## Findings

### Cross-flow analysis

#### [ ] READ-001 — Cross pass rebuilds `BoundFlowPlan` per (flow, module), duplicating the local projector's per-module plan

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:139,182-188,46-50`, `glass-lint-core/src/analysis/flow/planning.rs:317-323`, `glass-lint-core/src/analysis/flow/projector/mod.rs:129`

`CrossWorklist` owns `flow_plan_cache: HashMap<FlowPlanKey, BoundFlowPlan>`
keyed by `FlowPlanKey { flow, module }` (`cross/mod.rs:46-50,139`) and lazily
builds `BoundFlowPlan::single(flow_id, flow, names)` per reached `(flow,
module)` (`cross/mod.rs:182-188`). `BoundFlowPlan::single` is only called from
this one site. The local projector builds the *same* plan shape per module over
all roots with `BoundFlowPlan::new(rules, names)` (`projector/mod.rs:129`), so
for a module reached by `F` flows the cross pass constructs the plan-binding
indexes (`BoundTargetIndex` for sources/sinks, requirement member paths)
`F` times and one extra time redundantly versus the local pass. Every cross
query already filters by the context's `flow_id` (`matching_property_requirements`,
`matching_member_requirement_indices` in `planning.rs`, and the
`sink.flow_id() == self.context.state().flow_id()` filter in
`propagation.rs:173`), so a single all-flows plan per module yields identical
results.

**Recommendation:** Give plan construction a single owner — build one
`BoundFlowPlan` per module over all lifecycle roots (the same `roots` slice
`cross::collect` already receives), share it between the local projector and
the cross pass, and key the cache by `ModuleId`. Delete `BoundFlowPlan::single`
and `FlowPlanKey`. Guardrails: plans are per-module because they intern
`NamePath`s against the module's `NameTable`; the local projector keeps its own
execution state — only the immutable plan is shared; keep the flow-id filter on
cross sink selection so all-flow plans do not change certainty or evidence.

**Fix Applied:** None so far.

#### [ ] READ-002 — `value_root` identity-fallback lookup repeated across four sites

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/cross/sources.rs:166-168,183-185`, `glass-lint-core/src/analysis/flow/cross/worklist.rs:225-227`, `glass-lint-core/src/analysis/flow/cross/state.rs:319`

Four sites repeat the same lookup-and-fallback sequence:
`effect.value_root(v).unwrap_or_else(|| v)` (sources.rs twice,
worklist.rs once) and `effect.value_root(v).unwrap_or(v)` (state.rs:319).
`FunctionEffect::value_root` returns `Option<ValueId>` (`effect/mod.rs:131-133`),
so every caller must re-implement "treat unknown root as the value itself".
A future change to root semantics must touch all four locations independently.

**Recommendation:** Add a narrow inherent method on `FunctionEffect`
(e.g. `fn root_value(&self, value: ValueId) -> ValueId` that applies the
identity fallback) or a single cross-local helper, and replace all four
call sites. Guardrail: keep `value_root` returning `Option` if any future
caller must distinguish "no recorded root" from a real root; the helper is
only the totalized projection.

**Fix Applied:** None so far.

#### [ ] READ-003 — `ContextAdmission` is a parallel copy of `FifoAdmission` with a 1:1 mapping

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/cross/worklist.rs:18-23,89-94,107-113`

`ContextAdmission` (`Inserted`/`Duplicate`/`Full`) is a byte-for-byte
parallel of `BoundedFifo::FifoAdmission`, and `ContextWorklist::push`
(`worklist.rs:107-113`) exists only to match-map one enum onto the other.
Every production caller ignores the result (`enqueue_parameters`, the `seed*`
methods all discard `push`'s return); only `cross/tests.rs` asserts against
`ContextAdmission`. The second enum is pure boilerplate that must be kept in
lock-step with the first.

**Recommendation:** Delete `ContextAdmission`; have `ContextWorklist::push`
return `BoundedFifo`'s `FifoAdmission` directly (or return `()` and assert the
bound separately in tests). Guardrails: keep the `Duplicate` vs `Full`
distinction available to tests, since the doc comment explicitly distinguishes
deduplication from a rejected new context at the retained bound.

**Fix Applied:** None so far.

#### [ ] READ-004 — `EmissionContext` is an immediately-consumed facade over a slice of `CrossProjectionSession`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/cross/evidence.rs:171-175,196-210`, `glass-lint-core/src/analysis/flow/cross/propagation.rs:189-193,223-227`

`EmissionContext { project, evidence, arena }` is constructed identically at
the two `emit` call sites in `propagation.rs` from the fields of the
already-present `self.session` (a `CrossProjectionSession`), then immediately
destructured at the top of `emit` (`evidence.rs:204-208`). It forwards exactly
the wrapped fields with no added invariant or vocabulary, and exists only to
shrink `emit`'s parameter list.

**Recommendation:** Pass `&mut CrossProjectionSession` to `emit` (a child
module can already name `super::CrossProjectionSession`) and read
`session.project`/`session.evidence`/`session.arena`, deleting `EmissionContext`;
alternatively, derive the context from the session with one method so the two
call sites stop hand-building it. Guardrails: `emit` must not reach into the
session's `call_graph`, `worklist`, or `names`; evidence emission stays a
separate phase from worklist mutation.

**Fix Applied:** None so far.

#### [ ] READ-005 — `EvidenceKey` construction is duplicated and its invariants unencoded

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/cross/evidence.rs:45-50,160-169,213-217`

Both `mark_nonmatching` (`evidence.rs:162-167`) and `emit`
(`evidence.rs:212-217`) hand-build the same key literal —
`kind: MatchKind::CallArgument`, `symbol: flow.evidence_symbol().as_str().to_owned()`,
`fact: event` — and the test module does too (`evidence/tests.rs:48-52`). The
invariant "every cross-flow evidence key is a `CallArgument` key of the flow's
evidence symbol" is only implicit; a future change to evidence deduplication
must update all three sites.

**Recommendation:** Add a private constructor on `EvidenceKey`, e.g.
`EvidenceKey::for_call(flow: &CompiledObjectFlow, event: FactId)`, and route
all constructions through it. Guardrails: keep `kind`/`symbol` on the key
(they are still needed to merge into `RuleEvidenceTable`, and flows of the
same rule can share a symbol), but stop letting callers choose them freely.

**Fix Applied:** None so far.

#### [ ] READ-006 — `seed_from_calls` re-scans the candidate set per flow and per call argument

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/worklist.rs:228-264`

For every root call argument, `seed_from_calls` materializes
`sources.candidates(&source_key)` into a `Vec` and then, inside the per-flow
loop, calls `sources.candidates(&source_key).any(|item| item.flow_id() == flow)`
again for every flow in `source_flows` (`worklist.rs:250-253`). This is
O(calls × flows × candidates-per-key) repeated lookup over a collection that
was just materialized two lines above.

**Recommendation:** Compute the set of flows present at `source_key` once per
argument (or add `FlowSources::candidate_flows(&key)`), then iterate
`source_flows.difference(&present)` instead of re-scanning candidates per flow.
Guardrails: keep the `Duplicate`/unknown-source semantics — a call site with no
candidate still seeds an `unknown` context so incomplete alternatives downgrade
`Definite` to `Possible`.

**Fix Applied:** None so far.

#### [ ] READ-007 — `FlowLimits::from_flow_operations(x).operation_limit()` is an identity round-trip

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:271-272`, `glass-lint-core/src/analysis/model/flow/limits.rs:52,77-79`

`cross::collect` computes the cross budgets as
`FlowLimits::from_flow_operations(project.flow_limit()).operation_limit()`.
`from_flow_operations` stores `operations` verbatim (`limits.rs:52`) and
`operation_limit()` returns it unchanged (`limits.rs:77-79`), so the whole
`FlowLimits` construction is consumed immediately and contributes nothing.
The same cross-file comment style then uses this value for both
`source_budget` and `step_budget`.

**Recommendation:** Use `project.flow_limit()` directly for the two `Budget`s,
and delete the `FlowLimits` import/round-trip; if the intent is "cross flow is
bounded by the same operations budget as local flow", say so in a comment at
the construction site. Guardrails: `operation_limit` must keep matching the
value local flow charges against so a cross/local budget split cannot diverge.

**Fix Applied:** None so far.

#### [ ] READ-008 — `CallPropagation.module` always equals `context.module()`

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/propagation.rs:238-246,249-267`, call sites `glass-lint-core/src/analysis/flow/cross/propagation.rs:60-69`, `glass-lint-core/src/analysis/flow/cross/mod.rs:119-129`

`CallPropagation` stores a `module: ModuleId` field filled at both call sites
with `self.context.module()` (`propagation.rs:64`, `mod.rs:123`) and only reads
it as `self.module` while `self.context` is also held. The parameter duplicates
state already owned by the carried `CallContext`, and the constructor exposes a
freedom that no caller exercises.

**Recommendation:** Drop the `module` parameter and field and derive the module
from `self.context.module()` at the two read sites (`propagation.rs:279,291`).
Guardrails: propagation must always stay within the context's module — the
callee `ModuleId` for the `crossed` flag still comes from the call target, not
from `module`.

**Fix Applied:** None so far.

#### [ ] READ-009 — `ModuleEvidence::trace_heads` is a `pub(super)` counter field mutated and read across module boundaries

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/cross/evidence.rs:61-65,248-250`, `glass-lint-core/src/analysis/flow/cross/mod.rs:222-226`

`ModuleEvidence` owns `trace_heads: usize` as a `pub(super)` field that
`emit` increments by direct assignment (`evidence.rs:249`) and `CrossWorklist::finish`
reads by field access (`mod.rs:225`). The counter is storage-shaped
state whose increment rule ("count complete trace heads only") lives at the
call site instead of on the owning type.

**Recommendation:** Make `trace_heads` private and add narrow methods, e.g.
`fn record_trace_head(&mut self)` (only increments when the trace was
assembled) and `fn trace_heads(&self) -> usize`. Guardrails: keep the
`saturating_add` bound and the "only when `trace_head.is_some()`" rule inside
the owner.

**Fix Applied:** None so far.

#### [ ] READ-010 — `apply_property` and `apply_receiver` share the same requirement-advance skeleton

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/propagation.rs:86-119,121-154`

Both methods repeat the identical phase sequence — fetch the module fact
stream, clone `self.state`, read `self.flow.readiness()`, start with
`next.requirement_transition(readiness)`, merge `next.advance_requirement(...)`
per matching requirement of the context's flow, then
`self.emit_requirements(&next, event, transition)` and commit `*self.state =
next`. Only the "which requirements match" step differs
(`matching_property_requirements` vs `matching_member_requirement_indices`).
Maintenance risk: a change to the advance/commit protocol (e.g. the readiness
gating or emission policy) must be applied twice.

**Recommendation:** Extract one helper that takes the flow-relative requirement
match list (or a closure producing `Vec<RequirementIndex>`), and have both
methods supply only their matching step. Guardrails: keep `emit_requirements`'s
`CompletionMode::Configuration` + `is_crossed` gating in the shared path and
preserve the per-usage call-propagation ordering that `UsageProjector::project`
already enforces.

**Fix Applied:** None so far.

#### [ ] READ-011 — `ContextProjection` is a one-call-site borrow-packaging struct

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/mod.rs:71-131,197-205`

`ContextProjection` is constructed exactly once (`mod.rs:197-205`) to package
the borrows of a single work item (`session`, `context`, `effect`, `flow`,
`flow_plan`, cloned `state`, fresh `propagated` set). Its only behavior,
`project()`, delegates to `UsageProjector::project` and
`CallPropagation::propagate` in a fixed order. It exists to keep
`project_context` short and to avoid an argument list, adding a type without a
distinct invariant or vocabulary beyond "one context projection".

**Recommendation:** Inline the two-phase sequence into `project_context`
(keeping `project_usage` before `propagate_calls`), or — if the struct is
retained as the work-item bundle — document the ownership rule it enforces
(session is exclusive, state is clone-on-enter/commit-on-exit). Guardrails:
preserve the ordering contract and the per-context `state.clone()` + commit,
and the `through = None` propagation (versus the per-usage `Some(event)`
variant in `UsageProjector`).

**Fix Applied:** None so far.

## Systemic Themes

- **One borrow-packaging struct per pass:** `CrossProjectionSession`,
  `ContextProjection`, `UsageProjector`, `CallPropagation`, and
  `EmissionContext` each wrap `&mut` borrows of shared session state and are
  created at a single call site. This is a consistent, documented pattern that
  manages the borrow checker, but it layers three levels of delegation between
  a worklist item and the actual projection/emission, and it is where most of
  READ-004/008/011 land. A deliberate pass to flatten one layer (pass the
  session directly, delete one-call-site structs) would reduce indirection
  without touching behavior.
- **Repeated identity-fallback lookups:** beyond READ-002's `value_root`,
  the same "lookup, fall back to identity/empty" shape recurs for
  `module_fact_stream(...)`/`call_effect(...)` `Option` handling
  (`propagation.rs`, `sources.rs`, `worklist.rs`). The value_root case is the
  sharpest and is reported; the rest are guarded by `else { return; }`
  early-outs and are acceptable.
- **`pub(super)` storage access:** `ModuleEvidence.trace_heads` (READ-009) is
  the only raw-field leak; `SourceKey`/`SourceCandidate`/`CallContext` all
  expose narrow getters, and `FlowSources` keeps its maps private. That half of
  the chunk is already well encapsulated.
- **Good reuse:** `BoundedFifo` is shared correctly by both `FlowSources::propagate`
  and `ContextWorklist`, and `EvidenceTransition`, `FlowCompletion`, and the
  `crossed` boolean are the right granularity of shared state. No provider names
  or policy leak into core; the chunk stays provider-neutral.

## Open Questions

- **`FlowCompletionReason` headroom:** `FlowCompletion` stores a `u16` bitmask
  and `mark` does `1 << reason as u8` (`flow/mod.rs:54`). The `repr(u8)` enum
  currently has 15 variants (max index 14), so this is safe today, but the
  16-bit ceiling is an implicit, unenforced constraint: adding a 16th variant
  would shift past the mask width. A `u16::from(...)`/const-assert or a
  compile-time count check would make the bound explicit. Not reported as a
  finding because the constraint is currently satisfied.
- **Cross/local budget relationship:** READ-007 shows the cross pass derives
  `operation_limit` from the same flow budget as local flow, and
  `CrossProjectionOutcome.operations` is summed into `ProjectionOutcome`
  (`projection/outcome.rs:164`). Whether the two budgets should be one shared
  `Budget` or two deliberately independent limits is a policy decision the
  chunk documents only indirectly.

## Coverage

Audited `glass-lint-core/src/analysis/flow/mod.rs` (`FlowCompletion`,
`FlowCompletionReason`) and all of `analysis/flow/cross`: `mod.rs`
(`collect`, `CrossWorklist`, `CrossProjectionSession`, `ContextProjection`,
`CrossProjectionOutcome`, `FlowPlanKey`), `evidence.rs` (incl. `evidence/tests.rs`),
`graph.rs`, `propagation.rs`, `sources.rs`, `state.rs` (incl. `state/tests.rs`,
`SourceBudget`), `worklist.rs`, and `cross/tests.rs`. Traced callers and
neighboring owners in `analysis/project/projection.rs` and
`projection/outcome.rs` (the only production consumer of
`cross::collect`/`CrossProjectionOutcome`), plus the shared types in
`flow/planning.rs`, `flow/projector/mod.rs`, `flow/effect/mod.rs`, and
`model/flow/limits.rs`. Cross-rule workflow tests and fixtures were not part
of this chunk. `rg` signals used: `value_root(` for repeated lookup,
`CallPropagation::new|BoundFlowPlan::single|EmissionContext|EvidenceKey|ContextAdmission`
for duplicated construction, and `unwrap|expect|panic|dead_code` for panic and
leftover-architecture review (none found in production code).
