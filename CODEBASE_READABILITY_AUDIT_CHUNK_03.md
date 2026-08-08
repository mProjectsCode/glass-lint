# Codebase Readability Audit — Chunk 3

This audit covers Chunk 3 of `CODEBASE_STRUCTURE_CORE.md`: bounded local flow,
function effects and summaries, cross-call projection, planning, matching,
path overlays, and flow evidence. It is an architectural review only; no
source changes were made.

## Summary

The flow subsystem has the right overall constraints: it consumes the canonical
fact stream, keeps path alternatives correlated, bounds state and work, and
clears or downgrades results when analysis is incomplete. Readability risks
remain in the state-transition boundaries. Several helpers re-derive the same
call shape, undo and redo the same mutation algebra separately, and expose
control-stack operations that silently consume or ignore malformed frames. The
projector also concentrates most mutable flow concerns in one object, while
completion status and frozen/overlay path behavior are distributed across
multiple representations.

## Findings

#### [x] READ-001 — Call-effect accessors repeatedly rebuild one call shape

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / API
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:162-241`
- **Representative callers:** `glass-lint-core/src/analysis/flow/cross/sources.rs:250-275`; `glass-lint-core/src/analysis/flow/summary/sink.rs:232-252`; `glass-lint-core/src/analysis/flow/projector/evidence.rs:75-103`

`CallEffectRef::shape` already performs the canonical fact lookup, unwrap/path
selection, effective-argument extraction, and call-provenance projection.
However, `chain`, `rooted`, `result`, `provenance`, `global_name`, `target`,
and `effective_args` each call `shape()` independently. A single transfer
often asks for several of these fields, so the same fact is matched and the
same derived shape is rebuilt repeatedly. The accessors also make it harder
for a caller to see that all fields came from one consistent shape snapshot.

This is more than a small performance detail: call identity and effective
argument semantics are the boundary between retained facts and all local and
cross-call flow. Repeating the derivation in many consumers increases the
number of places that can accidentally select a different fallback or forget
the fail-closed `None` behavior.

**Recommendation:** Make one materialized `CallShape` the canonical internal
view for a call and expose its named accessors, or add a single scoped shape
query that callers retain while matching. Keep `CallEffectRef` as the
fail-closed lookup boundary and do not reconstruct syntax or resolution, but
centralize chain selection, rootedness, effective arguments, and provenance in
one derivation.

**Fix Applied:** `CallEffectRef::shape` is now the sole materialization boundary, and flow/linking consumers retain one `CallShape` while reading chain, rootedness, result, provenance, target, and effective arguments. Repeated ref-building accessors were removed while fail-closed fact lookup and chain fallback behavior remain unchanged. Verified with `make fmt && make ci`.

#### [x] READ-002 — Reversible flow mutations duplicate the entire delta algebra

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / State API
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:533-558,560-643`; `glass-lint-core/src/analysis/flow/projector/history.rs:14-87`
- **Representative callers:** `FlowStateTable::restore` passes `HistoryTransition::Undo` or `Redo` into separate `apply_inverse` and `apply_forward` matches.

Every `InverseDelta` variant is interpreted once in `apply_inverse` and again
in `apply_forward`. The two large matches must remain exact opposites for
aliases, state entries, requirement evidence, and sink evidence. The
`MutationLog` already owns the directional transition callback, but the
semantic meaning of each delta remains split across the table implementation.
Adding a new mutation requires editing both matches and reasoning about whether
the old/new payload order is reversed correctly.

This is a high-risk readability seam because rollback is the mechanism that
preserves path-local correlation. A missed inverse or forward case can make a
later path observe state from an incompatible branch while still producing a
well-typed result.

**Recommendation:** Put directional application on the delta/state owner, for
example as one `InverseDelta::apply(direction, state)` operation, or introduce
a private reversible-state adapter that owns both directions together. Keep
the bounded `MutationLog`, explicit transition failure, and deterministic
checkpoint semantics; do not replace correlated rollback with independently
joined maps.

**Fix Applied:** `InverseDelta::apply` now owns both undo and redo semantics in one directional transition, and `FlowStateTable::restore` delegates to it. Alias, state, requirement, and sink mutations retain their bounded log behavior and deterministic checkpoint semantics. Verified with `make fmt && make ci`.

#### [x] READ-003 — Control-stack mismatch operations consume or hide frames

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:800-905`; `glass-lint-core/src/analysis/flow/projector/control.rs:41-274`
- **Representative callers:** branch, switch, and try end transitions call `ControlStack::pop_region`; function exit calls `pop_function`; loop transitions use `loop_frame` and `pop_loop`.

`ControlStack::pop_region` pops the top frame before checking its region, then
returns `None` for a mismatch. `pop_function` has the same consume-before-check
shape. Callers in `control.rs` interpret `None` as “return without applying a
transition,” but the mismatched frame has already disappeared. Other methods
such as `take_loop_continues`, `new_loop_breaks_since`, and `route_abrupt`
return an empty/no-op result when the expected frame is absent. The protocol
therefore conflates a valid empty collection with a malformed or out-of-order
control event.

The canonical fact stream should make these states unreachable in normal
operation, but this is exactly the boundary that must fail closed when facts
are incomplete, unsupported, or exhausted. Consuming the wrong frame can
misassociate a later region or silently drop an abrupt exit, making the
projector's path completeness difficult to establish. The nearby
`unreachable!` branches also encode protocol assumptions outside the stack's
API.

**Recommendation:** Give the stack checked, non-consuming inspection/pop
operations that return a typed distinction such as `NoFrame`, `WrongRegion`,
or `WrongKind`. Only remove a frame after the expected kind and region match;
propagate mismatch into the projector's incomplete outcome rather than treating
it as an empty valid transition. Preserve normal LIFO control semantics,
bounded environments, and the existing fail-closed result policy.

**Fix Applied:** `ControlStack` now exposes checked, non-consuming operations
that distinguish empty, wrong-region, wrong-kind, and missing-target states.
The projector marks malformed control transitions incomplete instead of
silently dropping frames or treating missing loop state as an empty result.
Verified with `make fmt && make ci`.

#### [ ] READ-004 — ObjectFlowProjector owns too many mutable flow concerns

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** Complexity / Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:250-282,527-608,645-711,762-899`; related impls in `projector/control.rs`, `projector/evidence.rs`, `projector/loops.rs`, and `projector/transfer.rs`
- **Representative callers:** `collect_into` constructs one projector at `projector/mod.rs:203-218` and drives every fact through `ObjectFlowProjector::transfer`.

The projector is split into files, but one mutable object still owns the fact
stream and plan, function summaries, call indexes, live alias/state tables,
budget and exhaustion counters, control frames, path frontiers, pending
certainty groups, binding-slot representatives, evidence, trace arena, and
module identity. Its central methods then coordinate several of those domains
at once: `transfer_paths` restores every alternative and finalizes pending
evidence, `finish_loop` hands state through a fixed point and a control frame,
and `record_property_write` combines state mutation with plan-specific
matching and emission.

The file split improves navigation but does not establish ownership boundaries:
every helper impl can mutate the same frontier, control stack, run status, and
state table. That makes it difficult to audit whether a new transition preserves
the invariant that unknown, exhausted, or incompatible paths cannot establish
a witness.

**Recommendation:** Retain the projector as the one fact-stream orchestrator,
but move cohesive state transitions behind narrower private capabilities (for
example, a path machine, a reversible flow-state machine, and an evidence
emitter) with explicit transition results. Keep immutable inputs separate from
per-run mutable state where practical. Preserve one canonical traversal, loop
replay, path correlation, and the current bounded outcome accounting; this is
not a recommendation to introduce another semantic model or AST walk.

**Fix Applied:** None so far.

#### [ ] READ-005 — Flow completion and exhaustion status has no single internal contract

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:48-153`; `glass-lint-core/src/analysis/flow/effect/mod.rs:473-513`; `glass-lint-core/src/analysis/flow/summary/summaries.rs:27-79`; `glass-lint-core/src/analysis/flow/cross/mod.rs:132-246,249-292`; `glass-lint-core/src/analysis/flow/cross/sources.rs:214-225`
- **Representative callers:** local projection returns a bit-packed `LocalProjectionCompletion`; source collection returns `(FlowSources, bool)`; cross projection combines that bool with `WorklistStop`; summaries use `SummaryCompletion` and effects expose `budget_exhausted: bool`.

Each flow phase has a reasonable local representation, but phase boundaries
translate completion inconsistently. Local projection tracks seven exhaustion
bits, effects expose only a boolean, summaries carry one enum reason, source
propagation returns a bare boolean, and cross projection separately tracks a
worklist stop plus source-budget exhaustion. A caller must know which status
forms are authoritative and which detail has already been discarded.

The distinction matters architecturally: a mutation-log failure, source
propagation limit, invalid effect, trace-arena exhaustion, and alternative
truncation can all prevent definite coverage, but they do not necessarily mean
the same phase completed or that the same evidence should be retained. The
current representations make it easy for future code to merge a `bool` or
forget to carry a reason across a transition.

**Recommendation:** Define a private flow-completion value with explicit
`Complete`, `Incomplete(reason)`, and merge semantics, plus a bounded reason
set where multiple resources can exhaust. Use it at local, summary, source,
and cross-worklist boundaries while retaining phase-specific counters for
profiling. Preserve the existing possible-versus-definite policy and the
cross-pass behavior that clears evidence when the required complete analysis
did not finish.

**Fix Applied:** None so far.

#### [ ] READ-006 — Frozen and overlay summary paths repeat representation dispatch

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication / Domain API
- **Location:** `glass-lint-core/src/analysis/flow/summary/store.rs:5-228,244-261`
- **Representative callers:** parameter projection uses `matches_frozen`, `starts_with_frozen`, `without_first`, and `visit_segments` from `summary/parameter.rs:28-127`; sink projection creates joined overlay paths in `summary/sink.rs:267-284`.

`SummaryPathId` deliberately distinguishes immutable fact paths from bounded
overlay paths, but `SummaryPathStore` repeats that distinction in nearly every
operation: validity, depth, parent traversal, segment lookup, edge lookup,
append, and path reconstruction each branch on `Frozen` versus `Overlay`.
`starts_with`, `without_first`, and `visit_segments` then independently walk
the parent chain and rebuild segment vectors. The resulting API exposes a
representation-sensitive vocabulary to callers even though callers mostly
need path identity, prefix, projection, or iteration semantics.

The dual representation is justified by the freeze/overlay lifetime boundary,
but the repeated dispatch and reconstruction make path correctness hard to
review. In particular, callers must know which operations may allocate overlay
nodes (`join`) and which only find an already existing edge
(`rebuild_without_first`), while all failures are represented as `None`.

**Recommendation:** Keep `SummaryPathId` opaque and preserve the frozen versus
overlay identity check, but centralize parent/segment traversal in one private
path-walk abstraction and expose named domain operations for projection. Make
allocation versus lookup explicit in the operation names or result type, and
retain the overlay node bound and fail-closed invalid-path behavior.

**Fix Applied:** None so far.

#### [ ] READ-007 — Flow emitters must panic to use a fallible evidence constructor

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API / Error Handling
- **Location:** `glass-lint-core/src/analysis/flow/projector/evidence.rs:235-246`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:246-256`; constructor at `glass-lint-core/src/api/classification.rs:123-141`
- **Representative callers:** both local and cross-flow emitters always construct exactly one `ClassificationEvidenceOccurrence` and immediately call `.expect("flow evidence always has one occurrence")`.

The classification API correctly makes the general `from_occurrences` constructor
fallible when an empty evidence vector is invalid. The two flow emitters already
know a stronger invariant—one occurrence is built inline—yet they express it by
allocating a vector and invoking `expect`. This duplicates the same assertion
in separate flow paths and leaves a production panic at a boundary whose
overall policy is to model malformed or exhausted analysis as unsupported.

**Recommendation:** Add a named single-occurrence constructor, or a private
validated evidence builder that encodes the non-empty invariant without a
panic. Keep the general multi-occurrence constructor fallible and preserve
invalid rule-index handling, bounded evidence admission, and trace-arena
exhaustion behavior.

**Fix Applied:** None so far.

## Systemic Themes

- **ENCAPSULATE:** Control-frame validity, flow completion, and evidence
  construction need typed transition contracts instead of consuming operations,
  bare booleans, and panic-based assertions.
- **SIMPLIFY:** The projector's state ownership and transition orchestration are
  broader than the individual source files suggest.
- **DEDUPLICATE:** Call-shape derivation, reversible delta application, and
  frozen/overlay path traversal each repeat semantic dispatch at critical
  identity boundaries.

## Open Questions

None recorded.

## Coverage

Reviewed local object projection, path and state snapshots, mutation history,
control and loop handling, effect extraction, summary propagation, bound flow
planning, cross-call graph/source propagation, worklists, cross-flow state, and
evidence emission. `FlowMatchView::member_matches` intentionally permits a
final-segment match when full member paths differ
(`analysis/flow/planning.rs:82-85`). This is a deliberate flow-policy fallback,
not an accidental duplicate of exact path matching. Keep exact equality first,
retain the fallback, and add a focused positive/negative test and contract
comment; changing it would alter matching policy rather than readability.

## Handoff

Chunk 3 is reviewed. The next unreviewed chunk is **Chunk 4**; create
`CODEBASE_READABILITY_AUDIT_CHUNK_04.md` and review the retained models and
resolution boundaries listed in `CODEBASE_STRUCTURE_CORE.md`.
