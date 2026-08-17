# Codebase Readability Audit

Audit scope: Chunk 9 "Flow projector core" of `glass-lint-core`
(`src/analysis/flow/projector/`). Read-only; no source changed.

## Summary

The projector is well-factored at the domain level: `control.rs`, `loops.rs`,
`evidence.rs`, `transfer.rs`, and `history.rs` each own a real concern and
implement on `ObjectFlowProjector`, which is a legitimate aggregate. The
boundedness contract is coherent (single aggregation point
`FlowCompletion::from_sources` in `mod.rs`), and the path-token/batch
generation design is small and tested.

The findings below cluster around three weak spots:

1. **Duplicate bounded-restore protocol.** `restore_path` and `admit_path`
   (driver.rs) repeat the same charge → restore → mark-incomplete sequence and
   define two near-identical outcome enums (`PathRestoration`,
   `PathAdmission`). This is the clearest DEDUPLICATE win.
2. **A zero-method namespace struct.** `ProjectionPathMachine` groups four
   state areas but owns no operations; its invariants (batch generation,
   pending finalization, binding representatives) are all enforced by driver
   consumers reaching through `self.paths.*`.
3. **Bounded bookkeeping drift.** `MutationLog.charges` duplicates
   `ParentLinkedHistory::len()` and documents comparison charging that no code
   performs; `ControlFrame::Try` carries a `normal_exit` field that is written
   but never meaningfully read; a dead fallback consumes it.

Dependent ordering: all findings are independent; no fix blocks another.

## Findings

### Projector driver (`driver.rs`)

#### [ ] READ-001 — Restore and admit duplicate the bounded path-restore protocol

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/driver.rs:234-302` (with `PathRestoration` at `:37-41` and `PathAdmission` at `:28-34`; callers at `:97-101`, `:310-321`, and `loops.rs:93`)

`restore_path` (driver.rs:234-244) and `admit_path` (driver.rs:285-302) both
perform the identical charge → `flow_state.restore` → mark-incomplete sequence,
differing only in what happens after restoration (set `reachable` versus
insert into the `seen` snapshot set). The two outcome enums
`PathRestoration { Ready, Failed, Exhausted }` and
`PathAdmission { Admitted, Duplicate, RestoreFailed, Exhausted }` encode the
same restoration outcome under different names (`Failed` ≡ `RestoreFailed`),
so three matcher sites (`transfer_paths_with`, driver.rs:97-101;
`join_paths`, driver.rs:310-321; `LoopFixedPoint::collect_admitted`,
loops.rs:107-111) match on the same restore semantics through two type
systems. Any future change to the restore or budget behavior must now be
made twice.

**Recommendation:** Keep `restore_path` as the single owner of charge → restore
→ reachable bookkeeping, and implement `admit_path` on top of it: map
`RestorePath::Ready` to the `seen`-set check (yielding `Admitted`/
`Duplicate`), and map `Exhausted`/`Failed` to `PathAdmission::Exhausted`/
`RestoreFailed`. Delete the duplicated `charge_operation`/`restore`/mark body
from `admit_path`; `admit_path` keeps its signature and return enum, so callers
(`join_paths`, loops.rs) need no change. Guardrails: the delegated restore must
not disturb the post-pass `run.reachable` collapse that `join_paths` already
performs (driver.rs:329), `Exhausted` must still `break` out of both admission
loops, and `Duplicate`/`Failed` must still have no downward effect (no push of
an unreachable environment and no witness).

**Fix Applied:** None so far.

#### [x] READ-002 — `mark_incomplete` and `mark_control_stack_incomplete` are identical twins

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/driver.rs:65-71`

Both `pub(super) fn mark_incomplete` (:65-67) and
`pub(super) fn mark_control_stack_incomplete` (:69-71) are byte-for-byte
forwarders to `self.run.mark_incomplete()`. The shorter name has a single call
site (loops.rs:144), while the longer name covers 15 sites (12 in control.rs at
lines 51, 61, 71, 96, 105, 134, 149, 185, 218, 235, 247, and 272; driver.rs:203;
driver.rs:229; loops.rs:120); the semantic difference exists only in the name —
the completion model records the same `Alternatives` incompleteness for both.
Two entry points that degrade to the same state are a maintenance and naming
trap: a future fix that wants failures distinguished (e.g., a distinct
`FlowCompletionReason`) would have to migrate all 15 control-stack sites blind.

**Recommendation:** Keep one method (the shorter name `mark_incomplete`) and
delete the twin, updating the 15 `mark_control_stack_incomplete` call sites in
the same change. If control-stack failures should ever be distinguished from
budget failures, add a dedicated `FlowCompletionReason` bit at that point
rather than a parallel forwarding method. Guardrail: all call sites must still
degrade certainty to `Possible` (they all currently reach `run.mark_incomplete`).

**Fix Applied:** Removed the duplicate `mark_control_stack_incomplete`
forwarder and updated all control-stack callers to use the single
`mark_incomplete` method. The projector still records the same alternatives
incompleteness and therefore preserves the existing possible-certainty
behavior.

### Projector state machine (`mod.rs`)

#### [ ] READ-003 — `ProjectionPathMachine` is a named namespace, not an owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:191-213` (consumers: `driver.rs:90-117`, `driver.rs:332-358`, `driver.rs:403-420`, `control.rs` throughout, `loops.rs:154-177`)

`ProjectionPathMachine` groups four state areas (control stack, frontier,
pending flow states, binding representatives) and defines only `initial()`; it
implements no domain operation and hides no representation. Every invariant it
groups — active-batch generation/token math, pending finalization, binding
representative caching — is actually enforced inside its consumers that reach
`self.paths.frontier.*` / `self.paths.pending.*` / `self.paths.binding_slots`.
The struct therefore reads as a borrow-grouping container whose cohesion is
documented but never realized in a method surface. Because `ObjectFlowProjector`
already separates immutable inputs, tables, run counters, and path state, the
machine adds a level of indirection without an ownership or lifecycle boundary.

**Recommendation:** Move the per-path driving operations it should own onto the
machine — `begin_batch`/`select_path`/`end_batch`/`active_paths`/
`active_path` (currently standalone `PathFrontier` methods), plus
`finalize_pending` and `queue_state` (driver.rs:332-358) and the binding
representative lookup (driver.rs:413-417) — exposing narrow operations and
hiding the `active_batch`/`active_path` fields. Alternatively flatten the
struct onto `ObjectFlowProjector` if the methods would all need the projector
as a parameter (the `LoopFixedPoint` pattern). Keep the grouping only if it
gains operations. Guardrails: preserve the per-fact order restore → transfer →
capture → finalize-pending → join, and the strict generation-based rejection of
foreign path tokens (tests.rs:77-90).

**Fix Applied:** None so far.

#### [ ] READ-004 — `PendingFlowStateFinal` is a single-use intermediate that re-decomposes its key

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:318-382` (`PendingFlowKey` `:323-327`, `PendingState` `:329-333`, `PendingFlowStateFinal` `:337-342`, `finalize` `:344-377`)

The pending-state family stores `BTreeMap<PendingFlowKey, Vec<PendingState>>`
and the only consumer of `PendingFlowStateFinal` is `finalize_pending`
(driver.rs:332-345), which immediately destructures it. `PendingFlowStateFinal`
re-copies `key.event` as its own `event` field and drops `key.flow`, which is
re-derivable from any member `PendingState.state.flow_id()`. The three
associated types exist because `finalize` hands a grouped, certainty-tagged
result to the emitter on the projector; the key/state pair is genuine storage,
but the final wrapper is a transient tuple that re-states the key.

**Recommendation:** Fold `PendingFlowStateFinal` into the finalize returns —
either yield `(FactId, MatchCertainty, Vec<PendingState>)` per group, or have
`finalize` call back into a small `push_final(event, certainty, states)` method
so no parallel struct exists. Delete the wrapper type. Guardrails: certainty
must be computed once per `(flow, event)` group from active-path coverage and
`AlternativeCompleteness` (:360-369), and emission order must remain the
`BTreeMap` iteration order (deterministic by `(flow, event)`).

**Fix Applied:** None so far.

### Control-flow state (`state.rs`, `control.rs`)

#### [ ] READ-005 — `ControlFrame::Try::normal_exit` is written, cloned, and never meaningfully read

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:241` and `glass-lint-core/src/analysis/flow/projector/control.rs:196-258`

`normal_exit: Option<Vec<FlowEnvironment>>` is only assigned in `start_finally`
(control.rs:213, paired with `has_finally = true`) and is only read inside the
`else` branch of `end_try` (control.rs:254), which runs exclusively in the
`!has_finally` path where `normal_exit` is always `None`. `start_finally` also
pays an extra full `Vec` clone to populate it (`normal.clone()` at control.rs:213,
added on top of the necessary `try_exit.clone()` at control.rs:210 that makes
`normal` owned; `incoming = normal` is a move, not a clone), so every
`finally` block pays a `Vec` clone for a field that never affects output; the
finally exit logic is fully served by the
numeric `normal_count` and the `after[..normal_len]` slice (control.rs:239-251).
The `catch_exit.unwrap_or_else(|| normal_exit.unwrap_or_default())` fallback is
therefore dead defensiveness in a branch where both options are empty.

**Recommendation:** Delete the `normal_exit` field from `ControlFrame::Try`
(state.rs:241), along with its `None` initializer (control.rs:171) and its two
entries in the `Try` destructure patterns (control.rs:201, control.rs:228);
remove the assignment `*normal_exit = Some(normal.clone())` in `start_finally`
(control.rs:213); and collapse control.rs:254 to
`paths.extend(catch_exit.unwrap_or_default())`,
asserting the `!has_finally` pairing (e.g., `debug_assert!(normal_count == 0)`)
if desired. Guardrail: `normal_count` and the `after[..normal_len]` finally
indexing (control.rs:240-251) must remain untouched, and `try`/`catch` shapes
without `finally` must still fall back to empty vectors.

**Fix Applied:** None so far.

### Mutation history (`history.rs`)

#### [x] READ-006 — `MutationLog::charges` duplicates the log length and documents nonexistent comparison charging

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/history.rs:35-71` (used by `state/tables.rs:443-445`)

`MutationLog.charges` is incremented only inside `record` (history.rs:64-71), so
it always equals `history.len()`, which `ParentLinkedHistory` already exposes
(via `node_count`, history.rs:56-58). Its doc comment (:39-43) claims the count
"includ[es]... comparison charges" used to bound "CPU work from join
comparisons," but no code ever charges a join comparison to the log; comparison
work is tracked as a metric in `ProjectionRunState.coalescing_comparisons`
(driver.rs:314-317), while the join loop's total work is bounded by
`charge_operation` (mod.rs:482-489) — never by the mutation log. The comment
describes a protocol that does not exist, and the two mutable counters
(`charges` vs the log length) are two drift-prone witnesses of one value.

**Recommendation:** Delete `charges` and derive the bound directly from the
history length (`if self.history.len() >= self.limit { self.budget_exhausted =
true; return; }`), keeping `node_count` as the single canonical length. Fix the
comment to describe the actual mutation-output bound. Guardrail: the
`budget_exhausted` → `mutation_exhausted` → `FlowCompletionReason::MutationLog`
path (mod.rs:72-75, tables.rs:443-445) must keep producing the same
completion bit at the same threshold (`limit` inclusive).

**Fix Applied:** Removed the redundant `MutationLog::charges` counter and
made `record` enforce the mutation budget directly from the canonical history
length. Updated the stale comparison-charge implication with the simplified
implementation while preserving the inclusive exhaustion threshold and its
completion-reason path.

### Evidence emission (`evidence.rs`)

#### [ ] READ-007 — Ready-check-and-emit spine duplicated across the two emission helpers

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/evidence.rs:166-193` (`emit_completed_sink` `:166-178`, `emit_if_ready` `:181-193`)

`emit_completed_sink` and `emit_if_ready` share the identical spine — clone the
state for `(object, flow)`, look the flow up in the plan, evaluate a readiness
predicate, and call `emit_state` — differing only in the predicate
(`state.is_ready && state.sinks_ready` vs
`completion_mode == Configuration && state.is_ready`). A third site,
`record_helper_sink` (:150-163), repeats the `plan.get` + `is_ready` lookups
for the same purpose. Three copies of the state-clone/readiness protocol make
the emission contract (what "ready" means per mode) hell to change consistently.

**Recommendation:** Consolidate into a single emission helper on the projector,
e.g. `emit_if(flow, object, event, mode: Option<CompletionMode>,
require_sinks: bool)` (or one readiness struct), and route all three call
sites through it. Delete the two method bodies. Guardrails: certainty is not
assigned here — `emit_state` must still only queue via `queue_state`
(evidence.rs:198-203), replay suppression (:199) and the per-path finalization
path may not change, and the two predicates must remain distinguishable
(`sinks_ready` vs configuration-mode) in the unified signature.

**Fix Applied:** None so far.

## Systemic Themes

- **Domain partitioning is the strength.** `control.rs`, `loops.rs`,
  `evidence.rs`, `transfer.rs`, and `history.rs` each own a real concern and
  implement on `ObjectFlowProjector`; the cross-`impl` pattern is consistent.
  The aggregated boundedness contract (`FlowCompletion::from_sources`,
  mod.rs:54-89) is a legitimate multi-owner coordination point and should
  stay an associated function on `FlowCompletion`, not be flattened into one
  owner.
- **The weak ownership seam is "non-field state," not the projector shell.**
  `ProjectionPathMachine` and the pending-state family (READ-003, READ-004)
  are where the path machinery lives in name only; fixes should concentrate
  here rather than in `driver::*`, whose per-fact driver
  (`transfer_paths_with`, driver.rs:90-117) and loop coordinator
  (`finish_loop`, driver.rs:187-212) are appropriately sized for what they
  orchestrate once READ-001 removes the duplicated restore path.
- **`ProjectionRunState` vs `FlowStateTable`/`MutationLog`:** the split is
  defensible — the mutation log must live beside the tables it reverts so
  checkpoints stay local (tables.rs:84, :107), while scalar run/budget state
  lives in `run`. The real issue is not the split itself but the two
  duplicate witnesses it created (`MutationLog.charges` ≡ log length,
  READ-006) and limits fields handed to both owners
  (`FlowLimits` kept whole in `run`, two scalars re-extracted into
  `FlowStateTable`, driver.rs:54, tables.rs:100-110). Limiting that to one
  source (`run` owns `FlowLimits`; `FlowStateTable` accepts scalars — as it
  already does) is the narrow fix; do not collapse the two owners' lifetimes.
- **The reversible-transaction machinery is not duplicated.** Flow's
  `MutationLog` and scope's `OwnedHistory`
  (`analysis/scope/build/history.rs:54-103`) are separate, phase-specific
  wrappers over the same shared primitive (`ParentLinkedHistory` in
  `glass-lint-datastructures`); they differ in delta types, owner-guard and
  error semantics, and lifecycle. The primitive is already the single owner of
  the hard part. Unifying the wrappers is speculative unless the 
  scope checkpoints are moved in future chunk work.
- **Boundedness flags are spread by design** (`object_limit_rejected` in run,
  `state_limit_rejected`/`mutation_exhausted` in the table, `limit_rejected`
  in evidence, `TraceArena::is_exhausted`) and funnelled to one completion
  value; this aggregation point is the correct shape and is well documented.

## Open Questions (resolved)

- **Flow `MutationLog` vs scope `OwnedHistory` — resolved: the failure
  contracts are intentionally different; keep the owner-guard out of
  `ParentLinkedHistory`.** Scope runs many independent histories (`OwnedHistory`
  per `AssignmentEnvironment` and per `WriteSet`,
  `analysis/scope/build/history.rs:121-124` and `:247-252`), so a checkpoint
  captured against one history must be rejected when restored against another;
  the per-history `HistoryOwner` tag (`:24`, `:49-52`, `:55-58`) and the
  `ForeignCheckpoint`/`StateDesync` errors (`:36-40`, `:83-101`) catch exactly
  that. Flow owns exactly one `MutationLog` for the whole run
  (`FlowStateTable.log`, `tables.rs:84`, created in `FlowStateTable::new`,
  `tables.rs:107`), so every `FlowEnvironment` checkpoint necessarily comes from
  that same log and a foreign checkpoint is structurally impossible. The
  fail-closed `bool` (`history.rs:77-88`) is intentional: `restore_path` and
  `admit_path` already turn restore failure into `mark_incomplete` plus
  `Failed`/`Exhausted` (driver.rs:234-244, :285-302), so threading errors
  through the transfer loop would add nothing. Adding the guard to
  `ParentLinkedHistory` would cost a per-transition owner comparison on the hot
  restore path and force a signature change on both wrappers for no observable
  gain.
- **`PendingFlowStates::finalize` ownership — resolved: keep the returned,
  grouped result; within READ-004, prefer the "yield
  `(FactId, MatchCertainty, Vec<PendingState>)` per group" branch over the
  callback.** `finalize` is a pure transform (`&mut self` plus two value
  inputs, mod.rs:344-349) whose certainty derivation depends only on active-path
  coverage and `AlternativeCompleteness` (mod.rs:356-369). Returning the grouped
  result keeps that logic directly unit-testable and keeps evidence concerns out
  of the pending module, whereas a callback would couple pending storage to the
  projector's emission through a closure that only full runs could exercise.
  Confirmed that no unit test calls `finalize` or `queue_state` directly today
  — final certainty is exercised only through full runs
  (tests.rs:316-481, tests_extended.rs:261), which argues for keeping the pure
  return shape so the computation can be unit-tested without a run.
- **`ProjectionRunState.reachable` mirrors each environment's flag — resolved:
  keep it; it is a per-path live cursor, not a redundant store of snapshot
  truth, and it cannot diverge at any read site.** During `transfer_paths_with`
  the frontier is drained into a local `incoming` vec (driver.rs:91) and only
  refilled afterward (driver.rs:109), so the environment currently being
  transferred is not in the frontier; `run.reachable` is the projector's only
  signal of the current path's reachability, read at driver.rs:105 and via
  `transfer_call` (driver.rs:247) and `capture` (driver.rs:271). Every read is
  dominated by the `restore_path` write for that same path in the same
  iteration (driver.rs:242), plus the deliberate `= true` at a function `Enter`
  (driver.rs:222). Deleting it would mean threading per-path reachability
  through the transfer closure and into `capture` — a wide refactor with no
  boundedness or determinism win. Confirmed not a finding.

## Coverage

Chunk 9 sources reviewed (all files under
`glass-lint-core/src/analysis/flow/projector/`):

- `mod.rs` (493 lines): all types and the entry `collect_into`.
- `driver.rs` (461 lines): full driver loop, restore/admit, pending
  finalization, finish_loop, outcome assembly.
- `control.rs` (277 lines): branch/loop/switch/try transitions.
- `evidence.rs` (276 lines): record_configuration, record_sinks,
  record_helper_sink, emission helpers, trace building.
- `history.rs` (107 lines): `InverseDelta`, `Checkpoint`, `MutationLog`,
  `ReportEvidenceKey`.
- `loops.rs` (195 lines): `LoopFixedPoint` start/converge/collect_exits.
- `state.rs` (474 lines) and `state/tables.rs` (493 lines): `ControlStack`,
  `ControlFrame`, `AbruptExit`, `FlowEvidence`, `FlowStateTable`, snapshots.
- `transfer.rs` (69 lines): alias/value transfer and source matching.
- `tests.rs` (486 lines) + `tests_extended.rs` (324 lines): contracts for
  branch coalescing, loop fixed points, certainty, and budget exhaustion.

Related callers traced:

- `analysis/project/projection.rs:242-274` — drives `collect_into` per module.
- `analysis/project/projection/outcome.rs:139-158` — consumes
  `LocalFlowProjectionOutcome` metrics.
- `analysis/scope/build/history.rs:54-308` — sibling `ParentLinkedHistory`
  usage compared for DEDUPLICATE scoping.
- `analysis/flow/mod.rs:19-69`, `analysis/model/flow/limits.rs` — completion
  and limits contracts.

No source, test, configuration, or dependency files were modified; `git status`
shows a clean working tree before and after this audit apart from the addition
of this report.
