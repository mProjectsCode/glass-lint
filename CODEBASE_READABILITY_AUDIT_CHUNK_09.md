# Codebase Readability Audit — glass-lint-core Chunk 9: Flow projector core

## Summary

Chunk 9 is the `analysis::flow::projector` module: `mod`, `driver`, `control`,
`evidence`, `history`, `loops`, `state`, and `transfer`. It projects object
state through local control flow against the immutable fact stream, exposing a
single entry point (`collect_into`) to the sibling projection chunk
(`analysis/project/projection.rs`). The chunk's fail-closed lifecycle is in
good shape: every exhaustion point records into `FlowCompletion` /
`AlternativeCompleteness`, and pending-state certainty is derived from
active-path coverage rather than guessed. State ownership is also sound —
`FlowStateTable` owns alias/state/mutation-log invariants behind narrow methods,
and joins are centralized through checkpoint rollback.

The main readability problems are structural: (1) two parallel input
containers (`ObjectFlowProjectorInput` and `ProjectionInputs`) with five
overlapping fields, built and immediately destructured-rebuilt at a single call
site; (2) two near-duplicate path-transfer orchestrators in `driver.rs`;
(3) duplicated admission loops in `loops.rs`; (4) a `ControlStack::loop_frame`
that clones three environment vectors purely to satisfy a borrow, then
re-reads and pops the same frame; and (5) scattered direct writes to
`ProjectionRunState` invariants instead of a single method. Several redundant
conditionals and one unnecessary clone remain from refactoring.

No source, test, config, or documentation files were modified.

## Findings

### API surface and ownership

#### [ ] READ-001 — Parallel input containers with overlapping fields, consumed at one call site and immediately rebuilt

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/driver.rs:46-79`, `mod.rs:133-143,225-262`

`ObjectFlowProjectorInput` (driver.rs:46-56) is a nine-field bundle of
`pub(super)` fields constructed at exactly one call site (mod.rs:133-143) and
immediately destructured by `ObjectFlowProjector::new` (driver.rs:59-79), which
then re-bundles five of the nine fields (stream, names, plan, helpers,
module_id) into a second container, `ProjectionInputs` (mod.rs:225-235), via
`ProjectionInputs::new` (mod.rs:237-262). The two structs are parallel models
of the same construction: every immutable input appears in both, and the
destructure-rebuild sequence is a pure conversion path. The bundle exists to
pack arguments for `new`, so the struct's vocabulary adds no invariant beyond
"these are the inputs."

**Recommendation:** Delete the parallel conversion. Keep the
immutable-vs-mutable split (that is a real ownership distinction — the frozen
facts/plan inputs must not merge with the run-time evidence/limits/completion),
but make the construction path linear: either `ObjectFlowProjector::new` takes
the parameters `collect_into` already has (or a `ProjectionInputs` plus the
four run-state inputs) and `ProjectionInputs::new`'s `calls_by_result` scan
moves into the single construction site, or `ObjectFlowProjectorInput` absorbs
the `calls_by_result` index. Owner: `ObjectFlowProjector` / `collect_into`.
Guardrail: keep frozen inputs (`ProjectionInputs`) separate from per-run mutable
state (`ProjectionRunState`, `FlowEvidence`); do not collapse those ownership
domains.

**Fix Applied:** None so far.

#### [ ] READ-006 — `FlowEvidence` re-receives run-fixed bounds on every call; per-key cap is an unnamed literal

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:142-158`, `evidence.rs:247-253`

`FlowEvidence::record_if_admitted` (state.rs:142) takes `limit` and
`max_per_key` as parameters even though both are fixed for the run and the
type already tracks the relevant state (`total_emitted`, `emitted`). Its only
caller, `emit_state_final` (evidence.rs:247-253), passes
`self.run.limits.emission_limit()` and the literal `256` (evidence.rs:250)
each time. The magic `256` cap per evidence key is undocumented and easy to
lose across edits.

**Recommendation:** Store `emission_limit` and the per-key cap in
`FlowEvidence` at construction (`FlowEvidence::new`, called from
`ObjectFlowProjector::new` at driver.rs:73) and give the cap a named constant.
`record_if_admitted` then takes only the key and evidence. Owner:
`FlowEvidence`. Guardrail: the reserve/release rollback discipline
(state.rs:160-184) must be preserved; the per-key cap must remain bounded and
deterministic.

**Fix Applied:** None so far.

### Duplicated orchestration

#### [ ] READ-002 — `transfer_paths` and `transfer_paths_without_finalization` duplicate the path-transfer skeleton

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/driver.rs:102-127,249-266`

Both methods run the identical sequence: take the incoming paths, restore each
environment (break on `Exhausted`, skip `Failed`), run a per-path transfer,
collect reachable environments, `replace_paths`, `join_paths`, and
`end_batch`/re-join. They differ only in (a) `transfer_paths` selects a path
index via `select_path(path_index)` so pending states can be queued against
`PathToken`, and (b) `transfer_paths` calls `finalize_pending` before the join.
`transfer_paths_without_finalization` (used only by `transfer_function`'s
`Enter` branch, driver.rs:237) instead takes a `transfer: impl Fn(&mut Self)`
closure. The shared restore/charge/fail-closed handling is a single invariant
spread across two copies.

**Recommendation:** Collapse into one orchestrator that takes a per-path
closure plus a `finalize: bool` flag (or two small closure arguments for
"select path" and "transfer"); keep `transfer_function`'s enter semantics as
one caller. Owner: `ObjectFlowProjector` (driver). Guardrail: preserve the
`PathRestoration::Exhausted => break` ordering, the `select_path` call before
`transfer_fact`, and the `finalize_pending` placement relative to `join_paths`
— certainty and boundedness depend on them.

**Fix Applied:** None so far.

#### [ ] READ-004 — `loops.rs` duplicates the admit/deduplicate admission loop for replays and exits

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/loops.rs:88-105,161-169,179-187`

`admit_replay` (loops.rs:88-95) and `admit_exit` (loops.rs:98-105) are
identical except for which `BTreeSet<FlowSemanticSnapshot>` field is passed to
`projector.admit_path` (self.seen vs self.exit_shapes). Their call sites then
repeat the same four-arm `match PathAdmission` (Admitted -> push, Exhausted ->
break, Duplicate/RestoreFailed -> skip) once in `converge` (loops.rs:161-169)
and again in `collect_exits` (loops.rs:179-187). The `converge` loop also
repeats the `let Ok(...) else { complete=false; mark_control_stack_incomplete();
break }` error block three times (loops.rs:136-140, 144-148, 154-158).

**Recommendation:** Give `LoopFixedPoint` one `admit(projector, &mut seen,
environment)` method and one `collect_admitted(projector, &mut seen,
Vec<FlowEnvironment>) -> Vec<FlowEnvironment>` helper shared by the replay
frontier and the exit set; add a small `fail`/`incomplete` helper for the
repeated control-frame error blocks. Owner: `LoopFixedPoint`. Guardrail: keep
replay admission and exit admission on their own shape sets (they intentionally
converge on different collections), and keep every failure path setting both
`self.complete = false` and the projector's incomplete marker.

**Fix Applied:** None so far.

### Unnecessary work and dead logic

#### [ ] READ-003 — `ControlStack::loop_frame` clones three environment vectors just to satisfy a borrow, then the frame is re-read and popped

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:284-296`, `control.rs:106-120`, `driver.rs:197-230`

`ControlKind::LoopEnd` (control.rs:106-120) calls `ControlStack::loop_frame`,
which returns `Ok(frame.clone())` (state.rs:293), cloning the whole `Loop`
frame including `baseline`, `breaks`, and `continues` (three
`Vec<FlowEnvironment>`). The clone exists only so `finish_loop` can borrow
`self` mutably afterwards; the destructured Vecs are moved into `finish_loop`,
which then independently validates and pops the same frame via
`pop_loop(body_start)` (driver.rs:220). Every loop-end therefore copies all
live environments once and leaves the frame on the stack through the whole
fixed-point computation, only to pop it afterwards.

**Recommendation:** Add a `pop_loop_frame(region) -> Result<ControlFrame,
ControlStackError>` that moves the frame out (retaining `pop_loop`'s
`body_start` validation), destructure it in `transfer_loop`, and have
`finish_loop` stop popping. Delete `loop_frame`. Owner: `ControlStack`.
Guardrail: keep the region and `body_start` checks — popping the wrong frame
must remain a `mark_control_stack_incomplete` failure, not a panic.

**Fix Applied:** None so far.

#### [ ] READ-005 — Redundant conditionals in the transfer/record paths

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/transfer.rs:33-41`, `evidence.rs:74-98`

In `assign` (transfer.rs:33-41), the outcome of
`flow_state.admit_object(&aliases, object, states)` is inspected with
`if matches!(..., StateAdmission::Admitted) { return; }` followed by an
unconditional `return;` — both arms return, so the `matches!` can never
influence control flow and the admission result is discarded either way.
Rejection is already recorded on the table via `state_limit_rejected`
(tables.rs:364-367) and surfaces through `FlowCompletion::from_sources`
(mod.rs:63-66). In `record_sinks` (evidence.rs), each argument of the same call
re-runs `plan.sink_candidates_for_call(call)` (evidence.rs:79), and the inner
`if !matching_sinks.is_empty()` (evidence.rs:98) is dead because the collection
was built with `(!matching_sinks.is_empty()).then_some(...)` (evidence.rs:93).

**Recommendation:** Drop the `matches!` branch in `assign` (keep the call and
the `return`), hoist the single `sink_candidates_for_call` lookup above the
argument loop in `record_sinks`, and remove the redundant emptiness check.
Owner: `ObjectFlowProjector` (transfer/evidence). Guardrails: `admit_object`
must still be invoked (rejection must keep being recorded), and the
`StateAdmission`/`PathAdmission` variants used by joins and loops are
unaffected.

**Fix Applied:** None so far.

#### [ ] READ-007 — `finish_loop` clones the entrance paths and re-takes them from the frontier

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/driver.rs:206-209`

`finish_loop` does
`let mut entrance = frontier.take_paths(); entrance.append(&mut continues);
self.join_paths(entrance.clone()); let entrance = frontier.take_paths();`
(driver.rs:206-209). `entrance` is immediately shadowed by the re-take, so the
`.clone()` of the whole `Vec<FlowEnvironment>` (plus `continues`) at
driver.rs:208 is pure copying. The same `join_paths(...)` then
`frontier.take_paths()` store-then-re-read dance appears at driver.rs:124-125,
driver.rs:264-265, and control.rs:102-104.

**Recommendation:** Move `entrance` into `join_paths` (no clone), and consider
having `join_paths` return the joined `Vec<FlowEnvironment>` it already builds
so callers do not re-take it from the frontier. Owner: `ObjectFlowProjector`.
Guardrail: `join_paths` must keep its truncate-and-mark-incomplete behavior
(driver.rs:357-360) and its final `run.reachable` update; the returned set must
remain deduplicated and ordered.

**Fix Applied:** None so far.

### Encapsulation

#### [ ] READ-008 — `ProjectionRunState::alternatives_complete` is toggled by direct field writes from several modules instead of one method

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:485-493`, `driver.rs:85-87,273-275,327-330,358-360`, `loops.rs:124-127`

The "incomplete alternatives" invariant is written two ways. `mark_control_stack_incomplete` (driver.rs:85-87) centralizes the control-stack case, but the same sticky marker is assigned directly as
`self.run.alternatives_complete = AlternativeCompleteness::Incomplete` in
`ProjectionRunState::charge_operation` (mod.rs:489), `restore_path`
(driver.rs:273), `admit_path` (driver.rs:328), `join_paths` (driver.rs:359),
`finish_loop` (driver.rs:227), `replay_loop_body` (driver.rs:174,178), and
`loops.rs:126` — with `loops.rs` reaching into `projector.run.*` fields across
the module boundary. The invariant is never cleared, so a single guard method
would both document the contract and reduce the chance a future path forgets
to mark incompleteness.

**Recommendation:** Add a single `mark_alternatives_incomplete()` (or a
`mark_incomplete()` on `ProjectionRunState`) and use it everywhere; give
`LoopFixedPoint` narrow projector methods instead of direct `projector.run`
field access. Owner: `ProjectionRunState`. Guardrail: the marker must remain
sticky and must never be cleared mid-run; `FlowCompletion::from_sources`
(mod.rs:76-78) already reads it as the single source of the `Alternatives`
reason.

**Fix Applied:** None so far.

## Systemic Themes

- **Parallel outcome enums are intentional here.** `PathRestoration`
  (driver.rs:40-44) and `PathAdmission` (driver.rs:32-37) classify distinct
  phases (restore during transfer vs admission at joins/loops); they should not
  be merged. Kept as-is per the guardrail on distinct phases.
- **Fail-closed discipline is consistently enforced.** Every exhaustion and
  restore-failure path marks `AlternativeCompleteness::Incomplete` (or records
  a `FlowCompletion` reason), and pending certainty is gated on active-path
  coverage in `PendingFlowStates::finalize` (mod.rs:351-384). The audits'
  guardrail (unsupported/incomplete work stays distinct from successful-empty)
  is respected — e.g., `collect_into` returns an empty outcome for an empty
  catalog (mod.rs:125-127) while exhausted runs report incomplete.
- **Direct cross-module field access on the projector.** `loops.rs` and
  `control.rs` reach into `projector.run.*` / `projector.paths.control.*`
  directly; only some of these go through methods like
  `mark_control_stack_incomplete`. The mixed style (READ-008) is a recurring
  thread, not a one-off.
- **Collect-then-mutate traversal is repeated but locally owned.** The
  "collect keys/flows for an object, then mutate" pattern appears in
  `record_configuration` (evidence.rs:47-52), `apply_property_write`
  (tables.rs:278-281), and `remove_states_for` (tables.rs:451-456). It is
  currently kept within `FlowStateTable`'s owner where the mutation protocol
  lives; noted as a future consolidation target, not a finding, because the
  update shapes genuinely differ.

## Open Questions

- `coalescing_comparisons` (mod.rs:99, incremented at driver.rs:348-351) counts
  admitted paths after the first per join; the metric's precise definition is
  only visible in the increment site. If this counter is part of the profiling
  contract, its semantics could be documented on `LocalFlowProjectionOutcome`.
- The `replay_loop_body` mode toggle writes `self.run.emission_mode` directly
  (driver.rs:182-189) while `emit_state` reads it (evidence.rs:198). Whether a
  `EmissionMode` change should also suppress `queue_state`/pending finalization
  is not exercised by any test; current behavior relies on `emit_state`
  short-circuiting before `queue_state`, which holds under inspection.
- `LocalFlowProjectionOutcome` exposes five raw `pub` counter fields
  (mod.rs:92-104) consumed as a metrics DTO by
  `projection/outcome.rs:139-158`. Whether this should be a read-only
  accessor surface is a taste question; left open because the sibling chunk
  treats it as an aggregate.

## Coverage

Files reviewed:

- `glass-lint-core/src/analysis/flow/projector/mod.rs`
- `glass-lint-core/src/analysis/flow/projector/driver.rs`
- `glass-lint-core/src/analysis/flow/projector/control.rs`
- `glass-lint-core/src/analysis/flow/projector/evidence.rs`
- `glass-lint-core/src/analysis/flow/projector/history.rs`
- `glass-lint-core/src/analysis/flow/projector/loops.rs`
- `glass-lint-core/src/analysis/flow/projector/state.rs`
- `glass-lint-core/src/analysis/flow/projector/state/tables.rs`
- `glass-lint-core/src/analysis/flow/projector/state/tables/aliases.rs`
- `glass-lint-core/src/analysis/flow/projector/state/tables/updates.rs`
- `glass-lint-core/src/analysis/flow/projector/tests.rs`, `tests_extended.rs`, `state/tests.rs`
- Callers / supporting types: `analysis/project/projection.rs`,
  `projection/outcome.rs`, `analysis/flow/mod.rs`, `flow/planning.rs`,
  `model/flow/limits.rs`, `glass-lint-datastructures/src/budget.rs`

Search signals applied: `rg` for `collect_into|collect_with_limits`,
`ObjectFlowProjectorInput`, `loop_frame|snapshot_paths|clone()`, direct writes
to `run.alternatives_complete`, and `unwrap/expect/panic/unreachable`. All
`unreachable!` sites (control.rs:81,121,170,203,276; state.rs:374,400) are
single-level exhaustiveness guards behind the top-level `transfer_control`
dispatch and are appropriate; no panic on user input was found. Test-only
`unwrap`/`expect` in `tests.rs`/`tests_extended.rs` are assertion helpers and
not reported.

Final check: `git status --short` shows only this audit file as untracked; no
source, test, config, or Cargo files were modified.
