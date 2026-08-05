# Codebase Readability Audit — Chunk 2

## Summary

Chunk 2 covers the bounded local and cross-call flow pipeline: control-flow
projection, effects, summaries, planning, matching, and cross-file
propagation. The implementation has useful explicit bounds and deterministic
collections, but several invariants are carried by coordination code rather
than by the types that own the corresponding state.

The highest-value opportunities are to encapsulate control-stack and alias
lifecycle operations, make path-coverage state explicit, and centralize the
shared target/argument matching predicates without merging local and
cross-file state machines. The remaining findings reduce duplicated
construction/dispatch and context plumbing in effects and summaries.

No source, test, configuration, dependency, or documentation changes were
made by this audit.

## Findings

### [x] READ-007 — Make control-frame lifecycle an owned abstraction

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture / API design / hard-to-read control flow
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:512-558`,
  `projector/control.rs:43-371`, `projector/mod.rs:126-158`

`ControlFrame` contains the state needed for branch, loop, switch, try, and
function lifecycles, but the projector stores it as a raw
`Vec<ControlFrame>`. `control.rs` then repeatedly destructures frames,
checks regions, and searches the vector in reverse. In particular,
`transfer_abrupt` and `route_finally_abrupt` each implement nearest-loop or
nearest-switch routing, while `record_abrupt_exit` separately scans every
frame to record try exits. The same stack invariant is therefore distributed
across the enum layout, the vector owner, and several transfer functions.

The leaked invariant is not just stack order: an abrupt exit must be recorded
by the relevant nested `try` frames, routed to the nearest eligible loop or
switch, and remain correlated with the correct region and path environment.
Adding another control construct or changing finally behavior requires
coordinated edits in multiple places, and a mismatched region can otherwise
fall through as an ordinary path unless each caller preserves the implicit
checks.

Introduce a private `ControlStack` (or equivalent owner methods on the
projector state) that owns push/pop, region validation, abrupt-exit recording,
nearest-target lookup, and finally routing. Delete the raw reverse searches
and duplicated break/continue routing from `control.rs`; callers should ask
the owner for a domain operation and apply the returned path changes. Keep
the current fail-closed behavior for unsupported or mismatched control
states, nearest-target semantics for nested loops/switches, function-return
behavior, and deterministic path ordering. Tests should specifically cover
nested try/finally, break/continue through try blocks, returns, and region
mismatches.

**Fix Applied:** Added a private `ControlStack` owner for control-frame
storage, region validation, loop replay bookkeeping, function boundaries,
abrupt-exit recording, and nearest loop/switch routing. Projector control
transitions now use typed stack operations, and the duplicated abrupt-routing
and try-frame scans were removed while preserving fail-closed mismatches and
correlated path environments.

**Verification:** `cargo test -p glass-lint-core analysis::flow::projector --lib`
(50 passed); `make fmt && make ci` (passed).

### [ ] READ-008 — Co-locate alias binding and object-state cleanup

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture / ownership / API design
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:71-189,
  400-435`, `projector/mod.rs:618-758`, `projector/transfer.rs:19-46`

`FlowStateTable` owns the raw alias map, reverse reference counts, object
states, and mutation log. However, `ObjectFlowProjector::value_aliases`
expands a semantic value into several aliases (the value, its resolved ID,
and a binding-slot representative), while `bind_value`, `unbind_value`, and
`invalidate_object` decide when state is removed. The invariant that aliases,
reference counts, object states, and reversible history remain correlated is
therefore split between a storage type and its caller.

This is especially fragile around assignment: `transfer::assign` must
distinguish a known source from an unknown source, preserve sharing, bind all
semantic aliases, and remove an object only when its last alias disappears.
Future changes can update one alias candidate or remove states before the
other candidates have been processed. Such a failure would leave stale flow
state or erase a still-reachable object without an obvious type-level error.

Give a single private alias/object owner an operation that accepts the
projector's canonical alias set and performs binding, unbinding, refcount
updates, and orphan-state cleanup together. Keep binding-slot identity as a
separate input to alias expansion rather than making it a generic map
operation. Delete the projector's duplicated cleanup protocol once the owner
exists. Preserve unknown-source unbinding, shared-object behavior, lexical
version aliases, mutation-log capture/restore, state limits, and the
fail-closed handling of dynamic values. Add focused tests for multiple aliases,
last-alias removal, reassignment, and reversible history.

### [ ] READ-009 — Centralize flow target and argument matching predicates

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Architecture / duplicated semantic matching
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:113-248`,
  `projector/evidence.rs:30-110`, `projector/transfer.rs:53-89`,
  `effect/mod.rs:268-285`, `cross/propagation.rs:69-203`

Local projection, effect matching, and cross-call propagation independently
implement pieces of the same semantic selection: property-chain matching,
last-segment fallback, argument predicates, matching sink indices, and source
candidate filtering. For example, local evidence checks a member against a
full chain or its last segment, `transfer::match_source` repeats candidate
argument checks, and `UsageProjector::apply_property` and
`apply_receiver` repeat the corresponding cross-file checks. `BoundFlowPlan`
already owns target indexes and sink/source lookup, but not all of the narrow
predicate operations used by these consumers.

The duplication is risky because the local and cross paths can drift in their
handling of rooted versus heuristic chains, static versus dynamic arguments,
or source identity. The state machines should remain distinct, but selection
semantics are a shared rule. A rule change currently requires finding every
consumer and can yield inconsistent evidence or certainty between local and
cross-file analysis.

Add a narrow query/matching surface to `BoundFlowPlan` (or a private
`FlowMatchView`) for member, source, and argument selection. Reuse it from
the local projector and cross propagation, while leaving state transitions,
evidence recording, and local/cross certainty calculation in their owning
modules. Delete the repeated chain and argument predicate loops. Guard the
change with tests for rooted and fallback chains, aliases, dynamic arguments,
property writes, source identity, and independent possible/definite witnesses
across local and cross paths.

### [ ] READ-010 — Represent path coverage with a path-boundary type

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype / certainty API / state protocol
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:179-301,
  408-436, 670-679`, `projector/mod.rs:207-270`

`PathFrontier` and `PendingFlowStates` coordinate certainty using raw
`usize` indexes. `transfer_paths` sets `active_count` and `active_index`,
transfers one path, queues pending states with that index, and later asks
`PendingFlowStates::finalize` whether matching indexes cover every active
path. The relationship between those values is implicit: a pending entry is
valid only for the active path batch at one fact boundary, but neither the
type nor the API carries that generation or boundary.

The leaked invariant is central to the definite/possible contract. A future
change to batching, joining, replay, or pending-state lifetime could compare
indexes from different frontiers and incorrectly upgrade a possible finding
to definite, or suppress an independent complete witness. The raw index also
allows callers to queue state outside the active transfer protocol.

Introduce a private `PathBatch`/active-path token owned by the frontier, or
move transfer and pending finalization behind `PathFrontier` methods. A
semantic path-index newtype can supplement this, but the important ownership
boundary is that pending state cannot outlive or be compared with an unrelated
frontier generation. Preserve deterministic path order, duplicate-path
coalescing, incomplete alternatives, replay suppression, and retention of
independent complete possible witnesses. Add adversarial tests for joins,
replays, exhausted alternatives, and pending states across fact boundaries.

### [ ] READ-011 — Model incompleteness as a typed flow outcome

- **Severity:** Medium
- **Fix Complexity:** High
- **Theme:** SIMPLIFY
- **Category:** API design / distributed status aggregation
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:50-65,
  160-176, 333-340, 770-788`, `projector/state.rs:299-318, 388-412`,
  `projector/history.rs:36-85`, `analysis/flow/cross/mod.rs:51-57,
  106-211`, `summary/summaries.rs:19-26, 41-143`,
  `analysis/effect/mod.rs:533-574`

Budget and incompleteness state is represented as several booleans and
partially overlapping fields: local projection has `exhausted`,
`object_limit_rejected`, `summary_exhausted`, mutation/state/evidence limits,
and `alternatives_complete`; effects and summaries carry their own exhaustion
flags; cross projection combines return-budget and worklist stop reasons.
`ProjectionRunState::into_outcome` and `CrossWorklist::finish` each repeat
the policy of deciding whether incomplete work invalidates or clears
evidence. Callers must understand which flag means “no more work,” which means
“certainty is incomplete,” and which resource was exhausted.

The concern is maintenance complexity rather than a demonstrated current
semantic error. Distributed boolean aggregation makes it easy for a new
limit to affect the status bit but not evidence truncation, or for two layers
to encode the same exhaustion differently. It also obscures the important
distinction between a complete run with no findings and an incomplete run
whose findings can only be possible.

Define one flow completion/status domain containing the relevant reason set
and completeness semantics, and have subordinate components return that
typed status with metrics. Map it to public local/cross outcomes once at the
pipeline boundary; remove redundant booleans where they only mirror the
status. Preserve separate local, effect, summary, and cross resource metrics,
the distinction between complete and incomplete alternatives, evidence
clearing/fail-closed behavior, and deterministic reporting. Tests should
cover each limit independently and combinations of limits.

### [ ] READ-012 — Give function-effect construction and dispatch one owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Hard-to-read function / duplicated construction
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:302-311,
  589-715`

`FunctionEffectsBuilder::new` and the `Function Enter` branch in
`FunctionEffectsBuilder::consume` each construct a `FunctionEffect` by
spelling out the same fields. `consume` also combines function registration,
parameter-index construction, budget charging, fact dispatch, invalidation,
and unsupported-control handling in one long match. The result is a
single-level-looking function with several lifecycle responsibilities and a
duplicated initialization shape.

The owning abstraction is `FunctionEffect`: construction from program scope
and construction from function parameters should be named operations on that
type or its builder. Fact-specific transitions can then be split into private
handlers on the builder. Delete the duplicate struct literals and keep
registration separate from per-fact effect updates.

Preserve the program slot, enter-only parameter registration, source-order
effects, budget accounting, invalidation for unsupported/unknown operations,
and the existing treatment of returns, calls, property writes, and
assignments. Retain focused effect tests for function entry, parameter
aliases, unsupported facts, and budget exhaustion.

### [ ] READ-013 — Encapsulate summary invocation and path-projection context

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API design / context plumbing
- **Location:** `glass-lint-core/src/analysis/flow/summary/sink.rs:111-278`,
  `summary/summaries.rs:19-26, 103-197, 305-329`,
  `projector/evidence.rs:112-155`

`FunctionSummaries` owns the `FactStream`, `SummaryPathStore`, summary map,
and exhaustion state, but its operations expose that context as repeated
arguments. `FunctionSummary::is_invocation_compatible` and
`collect_sinks_for_call` accept stream/path data, while callers in
`FunctionSummaries` and `record_helper_sink` separately scan parameter
bindings, intern or join paths, clone summary data, and pass the same context
through multiple layers.

This storage-shaped API leaks the protocol for frozen versus overlay path
identity, parameter/default/rest compatibility, and bounded sink projection.
It invites a caller to use the wrong path store or omit a compatibility check,
and makes the same invocation plumbing harder to read in local helper-sink
and cross-summary paths.

Add a narrow summary context/view owned by `FunctionSummaries`, or move the
operations that require the store onto that owner. Expose semantic operations
such as compatibility checking and sink projection rather than raw stream and
path-store parameters; delete repeated external parameter/path plumbing after
migration. Preserve frozen-versus-overlay identity, rest/default/arity
semantics, rejection of spreads and dynamic roots, bounded overlay growth,
and isolation of incompatible paths. Tests should cover direct summaries,
helper sinks, and overlay projection with unknown or incompatible arguments.

## Systemic Themes

The dominant pattern is state protocol leakage. Raw vectors, indexes, maps,
booleans, and context parameters are passed between modules while the domain
invariants—control routing, alias reachability, path coverage, completion
semantics, and summary path identity—remain in callers. The code already has
good candidate owners (`BoundFlowPlan`, `FlowStateTable`, `PathFrontier`,
`FunctionSummaries`), so the recommended changes should remain narrow and
private rather than introduce a parallel analysis model.

Search signals used for this chunk included raw control-frame searches and
destructuring, `bind`/`unbind` and object cleanup paths, `active_index` and
`path_index` handling, exhaustion/rejection aggregation, repeated matcher
loops, and summary methods accepting stream/path context.

## Open Questions

- The control-stack refactor should be evaluated against the existing nested
  `try`/`finally` and abrupt-exit tests before deciding whether routing returns
  path environments or mutates the frontier through a callback.
- A typed completion domain should be shared only if its semantics fit effect,
  summary, local, and cross flow; otherwise the public flow boundary can own
  the aggregation while subordinate statuses remain private.
- Chunk 1 is the prior handoff. The next unreviewed handoff is Chunk 3:
  `analysis::local`, lowering, and matcher construction modules.

## Coverage

Reviewed every source file listed for Chunk 2 in `CODEBASE_STRUCTURE_CORE.md`:
all `analysis::flow::cross` modules, effect modules and tests, matcher,
planning, projector modules and tests, and summary modules. The review also
traced representative callers into the flow projection boundary where needed
to understand outcome ownership. No findings are marked applied.
