# Codebase Readability Audit

## Summary

Chunk 7 — cross-flow analysis (`analysis::flow` completion tracking and the
`analysis::flow::cross` overlay: `CrossWorklist`/`CrossProjectionSession`
session, `QualifiedCallGraph`, `FlowSources` candidate adjacency and
propagation, `ContextWorklist`/`BoundedFifo` worklist, `CrossFlowState`/
`CallContext` state, and the evidence builder). The layered design is
deliberate, not triplicated: `CrossProjectionSession` is a per-context borrow
bundle over `CrossWorklist` fields (required to split the `&mut` borrows),
`CallContext` is the queue item, and `CrossFlowState` is the monotone
per-context evidence state — each owns a distinct concern and `FlowCompletion`
merge/`mark_all_possible` downgrades track incompleteness correctly across the
source-propagation, step-budget, and context-capacity phases. The concrete
issues are one test-only `Budget` shim, a constant-and-unread evidence-key
field plus a doubled symbol allocation, a bounded generic FIFO sitting outside
the crate that owns bounded data structures, two overlapping one-shot
propagation coordinators sharing five fields, a context-matching free function
that delegates only to `CallContext` methods, and a module→effect→call
traversal skeleton repeated four times with a divergent invalid-effect gate.

6 findings: READ-001 — READ-006.

## Findings

### Cross-flow analysis

#### [x] READ-001 — `SourceBudget` is a `#[cfg(test)]`-only one-field shim over `Budget` whose tests re-test the library type

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/cross/state.rs:20-47`, `glass-lint-core/src/analysis/flow/cross/tests.rs:184-200`

`SourceBudget` (state.rs:20-27) wraps `Budget` in exactly one field and
forwards `new`/`try_charge`/`exhausted` verbatim (state.rs:32-47). It is never
used in production — the comment describes a "per-transfer budget", but the
real source-propagation budgets are plain `Budget` values driven directly in
`sources.rs:196` and `sources.rs:255-306` — and the only two consumers are
`source_budget_transfer_limit_is_detected` and
`source_budget_not_exhausted_after_stabilization` (tests.rs:184-200), which
exercise `Budget`'s own counter semantics through the shim. All nine
`propagate_*` tests in the same file construct `Budget` directly (tests.rs:34,
52, 71, 89, 103, 115, 135, 151, 171), so callers already demonstrate the shim
adds nothing.

**Recommendation:** Delete `SourceBudget` from `state.rs` and `use` it from the
two tests; rewrite those tests to construct `Budget::new(...)` and call
`try_push`/`exhausted` the way the neighboring `propagate_*` tests already do.
This also lets `state.rs` drop its `#[cfg(test)] use glass_lint_datastructures::Budget`
import. Guardrails: keep fail-closed exhaustion semantics unchanged (a
`try_charge` return of `false` still means the transfer budget is spent); note
`Budget`'s `try_push` is the established vocabulary in this module.

**Fix Applied:** Removed the test-only `SourceBudget` wrapper and changed its
two tests to use `Budget` directly with the established `try_push` vocabulary.
Exhaustion behavior and the production source-propagation budget are unchanged.

#### [x] READ-002 — `EvidenceKey.kind` is a constant, unread `MatchKind` field, and `emit` allocates the same evidence symbol twice

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/evidence.rs:45-60`, `evidence.rs:200-247`

`EvidenceKey` (evidence.rs:45-50) carries `kind: MatchKind`, but the only
constructor `for_call` hard-codes `MatchKind::CallArgument` (evidence.rs:55)
and no site ever reads the field — it participates only in the derived
`Eq`/`Ord` dedup identity and can never vary. Separately, `emit` allocates the
symbol twice per sink: once inside `EvidenceKey::for_call`
(`flow.evidence_symbol().as_str().to_owned()`, evidence.rs:56) and again in
`ClassificationEvidence::from_occurrence` (evidence.rs:237-240), even though
`evidence_symbol()` returns a `&SmolStr` and the item is then stored under the
key that already holds the same `String`, so the symbol is duplicated in both
the map key and the map value.

**Recommendation:** Delete the `kind` field and its `MatchKind` arm from
`EvidenceKey` (the `EvidenceKey::for_call` constructor becomes a
`Kind`-less key builder). In `emit`, hoist `let symbol =
flow.evidence_symbol().to_string()` once and pass a clone into both the key and
`from_occurrence`, removing the second allocation. Guardrails: keep the
dedup criterion exactly as-is — same `symbol` plus same sink `fact` across
flow roots of one rule must still merge in `ModuleEvidence::record`, and the
`MatchKind::CallArgument` value recorded on `ClassificationEvidence` itself (a
distinct field of the evidence type) must stay unchanged.

**Fix Applied:** Removed the invariant `kind` field from `EvidenceKey` and
changed `emit` to create the evidence symbol once before sharing it between
the deduplication key and classification evidence. The key’s symbol/fact
identity and the evidence’s `CallArgument` match kind are unchanged.

#### [x] READ-005 — `usage_matches_context` is a free function in the evidence module that only dispatches `CallContext`'s own match predicates

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/cross/evidence.rs:23-43`, consumer `glass-lint-core/src/analysis/flow/cross/propagation.rs:64-65`

`usage_matches_context(effect, usage, context)` (evidence.rs:23-43) lives in
`evidence.rs` but is only consumed by `UsageProjector::project`
(propagation.rs:64-65), and every arm calls back into `CallContext` state:
`matches_property_write` (evidence.rs:31-33), `matches_call_receiver`
(evidence.rs:34), or `effect.call_argument(...).is_some_and(|argument|
context.matches_argument(effect, argument))` (evidence.rs:35-41). It is dispatch over one
type's own matcher surface; the free-function form forces `evidence.rs` to know
`EffectUse`, `FunctionEffect`, and `CallContext` just to route three variants,
and leaves the "which `EffectUse` matches which context" contract undocumented
on `CallContext`.

**Recommendation:** Move the dispatch onto `CallContext` as an inherent method
(e.g. `CallContext::matches_use(&self, effect: &FunctionEffect, usage:
&EffectUse) -> bool`) next to the existing `matches_argument`/
`matches_call_receiver`/`matches_property_write` methods in `state.rs`, and
have `UsageProjector::project` call it. Guardrails: preserve the exact
per-variant semantics — property writes match by receiver-or-source-root,
call receivers match only the root parameter, and call arguments require the
parameter/root pairing already encoded in `matches_argument`; keep the
`EffectUse` extraction (`call_argument`) explicit so call-id/argument-index
resolution stays in one place.

**Fix Applied:** Moved usage dispatch onto `CallContext::matches_use` and
updated `UsageProjector` to call the owner method. The three existing matching
branches and call-argument lookup semantics are unchanged; the evidence module
no longer owns a `CallContext` dispatcher.

#### [x] READ-003 — `BoundedFifo`/`FifoAdmission` are reusable bounded data structures living outside the crate chartered to own them

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/cross/worklist.rs:18-76`, consumers `glass-lint-core/src/analysis/flow/cross/sources.rs:255-308` and `worklist.rs:85-118,156-254`

`BoundedFifo<T>` (worklist.rs:25-76) is a generic bounded deduplicating FIFO
with fail-closed exhaustion (`VecDeque` frontier plus `BTreeSet` seen-set,
`max_retained` bound, `exhausted` latch) used by two independent cross-flow
subsystems — the `ContextWorklist` and the `FlowSources` propagation frontier
(`BoundedFifo::<PropagationItem>` at sources.rs:256). That is exactly the
"reusable bounded data structure" the workspace chartered to
`glass-lint-datastructures`: its ARCHITECTURE.md states "every reusable bounded
data structure lives here" (citing the earlier READ-009 extraction of path and
table types) and its `budget` module already implements the homologous
exhaustion-latch `Budget`. Today the type is `pub(super)` inside `flow::cross`,
so a future second consumer cannot find or `use` it without widening core's
surface, and the `Inserted`/`Duplicate` variants of `FifoAdmission` are
actually consulted only by unit tests (production matches only
`FifoAdmission::Full`, sources.rs:260/293).

**Recommendation:** Move `BoundedFifo` and `FifoAdmission` into
`glass-lint-datastructures` behind a narrow public API (construct, `push` →
admission, `pop_front`/`take_pending`, `is_empty`, `is_exhausted` — `is_empty`
keeps the `propagate` loop at sources.rs:265 working), exporting `FifoAdmission`
with the documented `Inserted`/`Duplicate`/`Full` contract, and repoint
`worklist.rs` and `sources.rs`. Guardrails: keep `FifoAdmission` distinct from
`PathAdmission` and `StateAdmission` — the three classify different admissions
(queue capacity, path restoration, state batch) and the distinct vocabularies
are deliberate; keep `T: Ord + Clone`, the total-retained (seen-set) bound
rather than a pending-only bound, and the fail-closed behavior where `Full`
latches `exhausted` so downstream
`FlowCompletionReason::CrossContextLimit`/`SourcePropagation` marks are
preserved. Implementation order: before READ-006, which restructures callers in
the same files.

**Fix Applied:** Moved `BoundedFifo` and `FifoAdmission` into the public,
provider-neutral `glass-lint-datastructures` crate and added focused tests for
deduplication, total-retained bounds, exhaustion, and FIFO order. Core now
reuses the shared primitive while retaining its flow-specific
`ContextWorklist` wrapper and source-propagation behavior.

#### [x] READ-004 — `UsageProjector` and `CallPropagation` are overlapping one-shot coordinators sharing five of six fields

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/cross/propagation.rs:23-59,216-250`, constructions `propagation.rs:61-90` and `glass-lint-core/src/analysis/flow/cross/mod.rs:126-145`

`UsageProjector` (propagation.rs:23-59) holds `session`, `context`, `effect`,
`flow`, `flow_plan`, `state`, `propagated`, `stream`, and `matcher`;
`CallPropagation` (propagation.rs:216-223) holds `session`, `effect`,
`context`, `propagated`, `through`, and `state` — five shared fields and a
shared mutable `propagated` seen-set. The parallel struct is reconstructed
once per matching usage inside `UsageProjector::project`
(propagation.rs:67-75) and again as the post-projection pass in
`project_context` (mod.rs:137-145), each construction passing the same
session/effect/context/propagated/state by position, and `UsageProjector::new`
needs `#[allow(clippy::too_many_arguments)]` (propagation.rs:36). One per-context
projection therefore dresses the same five pieces in two wrappers, with
coherence resting on the two hand-written constructors — one of which carries
`#[allow(clippy::too_many_arguments)]` — rather than on a single shared owner.

**Recommendation:** Collapse the two coordinators into one per-context owner —
either make `CallPropagation`'s behavior inherent `propagate(through)` methods
on `UsageProjector` (renamed to communicate that a single context is being
projected) or, if separate phases are kept, remove the shared-field duplication
by having both borrow the same `ContextProjector` state instead of
re-passing five arguments. `project()` stays a thin loop over
`effect.uses()` calling `self.propagate(Some(event))` then the per-usage `apply_*`
step, and `project_context` calls `self.propagate(None)` after projecting.
Guardrails: preserve the exact ordering (per-usage propagation with the
pre-usage state via `through = Some(event)`, then the final `None` pass), the
`propagated` fact ordering used for deterministic enqueueing, and the
`target.module() != context.module()` crossed-flag computation so cross-file
`Possible`/`Definite` grading is unchanged.

**Fix Applied:** Moved call propagation onto `UsageProjector` and removed the
single-use `CallPropagation` wrapper. Per-usage propagation still runs before
the corresponding projection step, and `project_context` still performs the
final unbounded pass through the same owner, preserving the propagated-event
ordering and crossed-flag calculation.

#### [x] READ-006 — The module→effect→call traversal skeleton is repeated in four places with a divergent invalid-effect gate

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/cross/graph.rs:20-52`, `glass-lint-core/src/analysis/flow/cross/sources.rs:137-188` and `sources.rs:204-243`, `glass-lint-core/src/analysis/flow/cross/worklist.rs:191-254`

Four independent passes re-derive the same topology: `QualifiedCallGraph::build`
(graph.rs:22-50), `FlowSources::collect_candidates` (sources.rs:204-243),
`FlowSources::build_adjacency` (sources.rs:137-188), and
`ContextWorklist::seed_from_calls` (worklist.rs:198-254) each walk
`project.modules()` → `effects().iter_effects()` → `effect.calls()`. Three of
the four also resolve the caller's call shape via `stream.call_effect(event)`
(graph.rs:30, sources.rs:145, sources.rs:223); `seed_from_calls` needs no
shape. Target resolution diverges: `build` is the producer — it qualifies the
target itself via `project.qualified_function_target` (graph.rs:35-40) and
inserts it into the map — while `build_adjacency` (sources.rs:146-150) and
`seed_from_calls` (worklist.rs:204-208) read the same
`call_graph.get(QualifiedEvent::new(module, call.event()))` edge, and
`collect_candidates` resolves flows through the source index instead
(sources.rs:228-233). Three of the four skip `effect.is_invalid()` at the top
(graph.rs:25-27, sources.rs:141-143, sources.rs:218-220); `seed_from_calls`
omits that gate (worklist.rs:202-203) and only survives because invalid effects
never produce a qualified target, so the divergence both reads as intent and
costs nothing to fix — the classic signal of a skeleton that should exist once.

**Recommendation:** Add one shared qualified-call iteration owned by the graph
module (the natural owner of site→target qualification), yielding
`(module, effect, call)` with the per-call shape resolved, in deterministic
order, and drive `build`, `collect_candidates`, `build_adjacency`, and
`seed_from_calls` from it; each consumer then keeps only its distinguishing
logic, including target resolution — `build` must keep qualifying the target
into the map (the graph does not exist yet when it runs), while the other
passes keep reading `call_graph.get`, so the iterator should not attempt to
yield the target itself. Apply the invalid-effect gate uniformly inside the
iterator (including for seeding) and document it. Guardrails: preserve
deterministic iteration order, bounded memory (resolve shapes per call, never
collect the whole stream), the exact skip semantics for unresolved
targets/missing shapes (including in `seed_from_calls`, which today does not
resolve a shape), and the `seed_from_calls` distinction that unknown-source
alternatives must still be seeded for calls without a candidate.

**Fix Applied:** Added a graph-owned valid-call visitor, with a module-scoped
variant for the per-module source index. The call graph, source candidate and
adjacency passes, and context seeding now share the deterministic
module→effect→call traversal and invalid-effect gate while retaining their
consumer-specific shape and target handling. Context seeding still includes
unknown-source alternatives and stops at the same retained-context limit.

## Systemic Themes

- **The session/context/state layering is deliberate, not triplicated.**
  `CrossProjectionSession` (mod.rs:50-57) is a per-context borrow bundle that
  splits the borrows of `CrossWorklist.evidence` (`&mut`), `.worklist`
  (`&mut`), `.project`, `.call_graph`, `.names`, and `.arena` (`&mut`)
  (mod.rs:116-123);
  `CallContext` is the queue item; `CrossFlowState` is the monotone evidence
  state inside it. Each type owns a distinct role, so the chunk's candidate
  "three session books" concern is resolved — the real field overlap is the
  parallel `UsageProjector`/`CallPropagation` coordinators (READ-004).
- **Two-phase propagation is intentional.** `FlowSources::propagate` is a
  candidate-level fixed point over the adjacency index (sources.rs:255-308),
  and `ContextWorklist::seed` + `CrossWorklist::run` form a
  context-level traversal (worklist.rs:156-254, mod.rs:72-146). Both share the
  same bounded `BoundedFifo` primitive; that sharing (not a third custom queue)
  is the correct dedup, and it is what READ-003 relocates rather than rewrites.
- **`FifoAdmission`, `PathAdmission`, and `StateAdmission` are homologous but
  not interchangeable.** `Inserted`/`Duplicate`/`Full`,
  `Admitted`/`Duplicate`/`RestoreFailed`/`Exhausted`, and
  `Admitted`/`Rejected` classify different lifecycles (queue capacity,
  path restoration, state-batch admission). Do not merge them; the vocabulary
  difference is a feature the READ-003 guardrail preserves.
- **Incompleteness propagates fail-closed across phases.** `FlowCompletion`
  merges every exhaustion reason (mod.rs:156-161); when the merged state is
  incomplete, all cross evidence is downgraded to `Possible` via
  `ModuleEvidence::mark_all_possible` (mod.rs:157-161, evidence.rs:148-154), so
  no phase can claim definite coverage after a bounded resource gave out. This
  matches the architecture invariant "incomplete analysis never claims
  Definite" and must survive every refactor.
- **One bounded-primitive reminder:** the `u16` bit-set `FlowCompletion` + the
  `Budget` latch pattern now appear in four flow modules (cross, summary,
  projector, effect) with compatible semantics but separate vocabulary; worth
  keeping an eye on as a future `glass-lint-datastructures` consolidation
  candidate, not a finding here.

## Open Questions (resolved)

- **`BoundedFifo`'s `Inserted`/`Duplicate`/`Full` contract (resolved):** the
  three-variant admission is intentional and should be kept. Production callers
  consult only `Full` (sources.rs:260-261, 293-295; the `ContextWorklist::push`
  results are discarded at worklist.rs:146, 176-187), and the unit tests are
  what distinguish `Inserted`/`Duplicate`/`Full` to prove the total-retained
  bound (tests.rs:234, 244, 251-258). The variant set is the documented
  admission contract for READ-003's relocation; production callers should keep
  latching on `Full` (fail-closed) and may treat `Duplicate` as success — no
  caller today needs to react to deduplication.
- **Per-phase `Budget` accounting (resolved):** the separation is deliberate and
  must be preserved. There are two `Budget` instances — `source_budget` for the
  `FlowSources` propagation fixed point (mod.rs:213-215) and `step_budget` for
  the context traversal (mod.rs:217, 79-81) — both seeded with the same
  `project.flow_limit()` (mod.rs:212, 217). The READ-006 walking passes are not
  budget-charged: `build`, `collect_candidates`, and `build_adjacency` are
  project-size-bounded walks, and `seed_from_calls` is bounded by the
  worklist's `MAX_CONTEXTS` (worklist.rs:161). Merging the budgets would couple
  the phases (source exhaustion would starve context stepping and vice versa)
  and erase the per-phase attribution carried by `SourcePropagation` vs
  `CrossStepBudget` in `FlowCompletion` (flow/mod.rs:41-42); only
  `step_budget.used()` is reported (mod.rs:177).
- **`collect` naming its `ExportLookupCache` parameter `session` (resolved):**
  cosmetic, worth aligning when READ-004/READ-006 touch these signatures. The
  parameter is passed straight to `QualifiedCallGraph::build` (mod.rs:210,
  graph.rs:20) and used only for target qualification (graph.rs:35-40), so it
  should be renamed to `cache` (or `exports`) to end the collision with the
  `CrossProjectionSession` type in the same module (mod.rs:50-57).

## Coverage

Files reviewed for this chunk (Chunk 7, cross-flow analysis):

- `glass-lint-core/src/analysis/flow/mod.rs` (`FlowCompletion`,
  `FlowCompletionReason`)
- `glass-lint-core/src/analysis/flow/cross/mod.rs` (`CrossWorklist`,
  `CrossProjectionSession`, `CrossProjectionOutcome`, `collect`, `run`,
  `project_context`, `finish`)
- `glass-lint-core/src/analysis/flow/cross/graph.rs` (`QualifiedCallGraph`,
  `QualifiedCallSite`, `build`, `get`)
- `glass-lint-core/src/analysis/flow/cross/sources.rs` (`FlowSources`, `SourceKey`,
  `SourceCandidate`, `PropagationItem`, `collect_candidates`, `build_adjacency`,
  `propagate`)
- `glass-lint-core/src/analysis/flow/cross/state.rs` (`CrossFlowState`,
  `EvidenceTransition`, `CallContext`, `CallContextOrigin`, `SourceBudget`)
- `glass-lint-core/src/analysis/flow/cross/worklist.rs` (`BoundedFifo`,
  `FifoAdmission`, `ContextWorklist`, `seed`, `seed_from_sources`,
  `seed_from_calls`, `enqueue_parameters`)
- `glass-lint-core/src/analysis/flow/cross/propagation.rs` (`UsageProjector`,
  `CallPropagation`)
- `glass-lint-core/src/analysis/flow/cross/evidence.rs` (`EvidenceKey`,
  `RuleEvidence`, `ModuleEvidence`, `usage_matches_context`, `emit`,
  `mark_nonmatching`, `assemble_trace`)
- Tests: `cross/tests.rs`, `cross/evidence/tests.rs`, `cross/state/tests.rs`,
  `flow/tests.rs`
- Lifecycle/callers traced: `analysis/project/projection.rs`
  (`collect_cross`, lines ~125-134), `analysis/project/projection/outcome.rs`
  (`record_cross`), `analysis/flow/projector/{mod,driver}.rs`,
  `analysis/flow/summary/summaries.rs` (completion-merge pattern),
  `analysis/flow/effect/mod.rs` (completion producers),
  `analysis/flow/planning.rs` (`BoundFlowPlan`, `BoundSource`, `BoundSink`),
  `api/classification.rs` (`ClassificationEvidence`,
  `EvidenceKey`-adjacent table record paths).

Read-only audit; no source files were modified. `git status` confirms the only
new file is this audit.
