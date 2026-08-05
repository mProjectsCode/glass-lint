# Codebase Readability Audit — Chunk 8

## Summary

Chunk 8 covers effect records, pre-bound flow plans, and the local object-flow
projector's state, history, and loop fixed-point types. The types make several
important invariants visible: effect summaries are invalidated on unsupported
control flow, plans bind symbol paths once, projector environments use
checkpointed state, and loop shapes normalize projection-local object IDs.

The main readability risks are ownership boundaries that remain distributed:
event extraction and flow-transition matching are repeated in cross-flow
callers, source indexes are rebuilt beside the bound plan, alias reference
counts are replayed by a second module, and loop admission has both duplicated
wrappers and a misleading failure variant. These are concrete maintenance
risks because a new effect kind, flow matcher, or bounded-state outcome must be
updated in multiple places while preserving fail-closed certainty.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### Effect-use identity

#### [x] READ-041 — Let `EffectUse` own its event identity

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API / Deduplication / identity ownership
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:61-78`,
  `analysis/flow/cross/evidence.rs:24-30`,
  `analysis/flow/cross/propagation.rs:35-65`

Every `EffectUse` variant stores an event, but the owning enum exposes no event
accessor. `effect_use_event` in cross-flow re-matches all variants to extract
that identity, and `UsageProjector::project` immediately performs another
variant match to extract the same event before dispatching the use-specific
operation. The event is the stable identity used for propagation and evidence,
so keeping its extraction in callers makes a new use variant easy to add to one
match and forget in another.

Add an `EffectUse::event()` method (and, if needed, a narrow typed view for the
variant-specific payload) and route propagation/evidence through it. Delete
the free event extractor and the duplicated event-only branches after callers
are migrated. Preserve the current per-variant payload dispatch, deterministic
use order, event anchoring, and the rule that an unsupported effect never
becomes a cross-module witness.

**Fix Applied:** Added `EffectUse::event()` as the single event-identity
accessor. Cross-flow propagation now obtains the event once for propagation
and variant-specific application, removing the free extractor and duplicated
event-only matching while preserving each use variant's payload dispatch.

**Verification:** `cargo test -p glass-lint-core analysis::flow::cross --lib`
(17 passed); `make fmt && make ci` (passed).

### Flow-plan and local/cross projection boundaries

#### [x] READ-042 — Reuse one bound source-index construction path

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture / Index ownership
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:48-76,113-148`,
  `analysis/flow/cross/sources.rs:190-207,223-260`

`BoundFlowPlan::new` constructs and normalizes a `BoundTargetIndex` for local
`BoundSource` values. Cross-flow then defines `SourceIndex` as the same generic
index and independently walks every compiled flow, resolves each lifecycle
target, inserts flow IDs, and normalizes the vectors in `build_source_index`.
The two paths therefore duplicate the target-binding and deterministic
normalization protocol. A lifecycle-target representation or normalization
change must be kept in sync even though both indexes are built from the same
compiled source declarations.

Move the shared “compiled sources to bound target index” traversal into the
planning owner, parameterized only by the value produced for each source (or
provide a plan constructor/view for the cross-flow `FlowId` index). Keep the
local `BoundSource` argument constraints and the cross-flow ID-only lookup as
separate value policies. Preserve missing-name fail-closed behavior, B-tree
ordering, deduplication, per-module lookup, and local/cross flow identity.

**Fix Applied:** Added a planning-owned generic `build_source_index` that
performs lifecycle target binding, insertion, and deterministic normalization
once. Local plans provide `BoundSource` values and cross-flow provides `FlowId`
values, preserving their separate lookup policies without duplicating the
compiled-source traversal.

**Verification:** `cargo test -p glass-lint-core analysis::flow::cross --lib`
(17 passed), `cargo test -p glass-lint-core analysis::flow::summary --lib`
(35 passed); `make fmt && make ci` (passed).

#### [x] READ-043 — Centralize local and cross-flow transition matching

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Architecture / Matcher ownership / API
- **Location:** `glass-lint-core/src/analysis/flow/projector/evidence.rs:30-110`,
  `analysis/flow/cross/propagation.rs:69-170`

The local projector and cross-flow `UsageProjector` independently implement
the same three semantic transitions. Local `record_configuration` and cross
`apply_receiver` both compare a member chain (including the last-segment
fallback) and run argument predicates. Local property-write handling and
cross `apply_property` both select property-write requirements and compare the
static value. Local `record_sinks` and cross `apply_argument` both resolve a
call target, select matching sink indices, and record them before testing
completion. The surrounding state differs (`FlowState` versus
`CrossFlowState`), but the matcher decisions and their ordering are shared
policy.

Extract pure plan-owned transition matchers that return requirement/sink
indices (and the relevant value-precision result), then let each projector
apply those indices to its own state and certainty/emission policy. Delete the
parallel chain/property/sink predicates after migration. Preserve local alias
reachability, qualified cross-module events, crossed-only cross-flow emission,
last-segment matching, argument constraints, and conservative possible versus
definite outcomes.

**Fix Applied:** Added pure `BoundFlowPlan` matchers for member-call
requirements and property-write requirements. Local and cross-flow projectors
now consume those indices and value-match results while retaining ownership of
their distinct state, event qualification, and emission policies; sink index
selection remains on the same plan boundary.

**Verification:** `cargo test -p glass-lint-core analysis::flow::projector --lib`
(52 passed), `cargo test -p glass-lint-core analysis::flow::cross --lib`
(17 passed); `make fmt && make ci` (passed).

### Projector state and history

#### [x] READ-044 — Make alias reference-count replay an owned state transition

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** State ownership / History / invariant
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:103-188`,
  `analysis/flow/projector/history.rs:78-189`

`FlowStateTable` owns the alias map and `ObjectRefCounts`, but alias mutation
logic is split between its forward methods and the history module's
`apply_inverse`/`apply_forward` functions. `bind` and `unbind` update the map,
record an `InverseDelta`, and adjust counts; the replay functions independently
insert/remove aliases and manually increment/decrement the same counts for
each delta variant. The invariant that `ObjectRefCounts` exactly reflects the
current alias map is therefore maintained by two switch statements in
different modules. A new alias delta or changed rollback behavior can leave
the object liveness index inconsistent even while checkpoint restoration
reports success.

Give the state owner one alias transition primitive (or an owned alias-table
newtype) that updates the map and counts together, and have history replay call
that primitive while supplying only the direction and delta. Remove the
parallel alias bookkeeping from `apply_inverse`/`apply_forward`. Preserve O(1)
checkpoints, parent-linked branch transitions, object liveness cleanup, and
the bounded/incomplete result when mutation history is exhausted.

**Fix Applied:** Added an `AliasTable` owned by `FlowStateTable` that couples
the alias map and reverse object-reference counts. Forward binding and history
replay now use its shared set/remove transitions, leaving `MutationLog` to
apply deltas without duplicating reference-count bookkeeping.

**Verification:** `cargo test -p glass-lint-core analysis::flow::projector --lib`
(52 passed); `make fmt && make ci` (passed).

### Loop fixed-point admission

#### [x] READ-045 — Name loop restoration failures separately from unboundedness

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** API / Bounded-state diagnostics
- **Location:** `glass-lint-core/src/analysis/flow/projector/loops.rs:18-29,89-106`

`LoopAdmission::Unbounded` is returned when `FlowStateTable::restore` fails.
That condition means a checkpoint/history transition could not be restored; it
does not establish that the loop generated an unbounded semantic frontier.
The same variant is then grouped with operation exhaustion to mark the fixed
point incomplete. The name makes a future diagnostic or recovery branch
attribute a history/state failure to loop growth, obscuring which bound or
invariant actually failed.

Rename the variant to reflect the failed transition (for example,
`RestoreFailed`) and reserve an unbounded/limit outcome for an actual loop
bound rejection, or introduce a typed admission error carrying the reason.
Keep all such failures incomplete and fail-closed; preserve duplicate-shape
convergence, operation charging, and the rule that no failed restoration can
produce a witness.

**Fix Applied:** Renamed `LoopAdmission::Unbounded` to
`LoopAdmission::RestoreFailed` and updated its documentation and fixed-point
handling. Restoration failures remain incomplete and fail closed, while the
name no longer attributes them to loop growth.

**Verification:** `cargo test -p glass-lint-core analysis::flow::projector --lib`
(52 passed); `make fmt && make ci` (passed).

#### [x] READ-046 — Share replay and exit admission bookkeeping

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** State transition / Fixed point
- **Location:** `glass-lint-core/src/analysis/flow/projector/loops.rs:89-142`

`admit_replay` and `admit_exit` both call `admit_into`, then repeat the same
test for `Exhausted | Unbounded` and the same `self.complete = false` update.
Only the destination shape set differs (`seen` versus `exit_shapes`). This
splits the admission protocol across two wrappers and makes changes to what
counts as an incomplete fixed point prone to drift between replay and exit
paths.

Use one owner-level admission operation that accepts the destination shape set
or a small admission collector and applies the shared completion transition;
keep replay and exit methods as naming-level call sites if they improve the
fixed-point narrative. Preserve separate replay/exit deduplication sets,
semantic snapshot normalization, operation limits, and deterministic exit
ordering.

**Fix Applied:** Moved the shared admission transition into
`LoopFixedPoint::admit_into`. Replay and exit admission now select their
destination shape set while one owner applies charging, restoration failure,
and fixed-point incompleteness consistently.

**Verification:** `cargo test -p glass-lint-core analysis::flow::projector --lib`
(52 passed); `make fmt && make ci` (passed).

## Systemic Themes

Chunk 8's strongest types are the private `BoundTargetIndex`, checkpointed
`FlowStateTable`, and canonical `FlowSemanticSnapshot`; they provide useful
determinism and bounded-state foundations. The remaining risk is that the
protocols around them are distributed: effect identity is extracted by
callers, local and cross projection repeat matcher semantics, source indexes
are rebuilt in separate modules, alias counts are replayed outside their
owner, and loop admission status is duplicated or misnamed.

Refactors should centralize pure matching and state transitions without
collapsing local fact state into cross-module overlays. They must retain
artifact-local identities, independent possible witnesses, deterministic
ordering, and conservative incomplete outcomes under unsupported semantics or
resource exhaustion.

Search signals used for this chunk included repeated enum matches for the same
event, repeated target-index normalization, parallel local/cross requirement
and sink predicates, alias/refcount updates in both mutation and replay paths,
and fixed-point admission wrappers with identical stop handling.

## Open Questions

- The shared source-index helper should preserve the distinction between local
  `BoundSource` values with argument constraints and cross-flow `FlowId` values;
  only target binding and normalization should become common.
- Local and cross transition matchers need a shared pure result API, while
  state mutation, qualified-event construction, crossed-only emission, and
  certainty remain owned by their respective projectors.
- The next unreviewed handoff is Chunk 9: flow summaries and local lowering.

## Coverage

Reviewed the Chunk 8 types listed in `CODEBASE_STRUCTURE_CORE.md` across
effect records and builders, bound planning indexes, the object-flow
projector, projector state/history/evidence, and loop fixed-point modules,
with representative callers in local projection and cross-flow propagation.
Existing Chunk 1–7 findings were checked to avoid re-reporting fact traversal,
flow-control stack protocols, generic path-coverage/exhaustion state,
cross-flow state emission ownership, trace assembly, and fact-table pairing.
No findings are marked applied.
