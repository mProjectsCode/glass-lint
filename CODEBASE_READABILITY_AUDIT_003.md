# Codebase Readability Audit — Chunk 03

## Summary

Chunk 03 owns bounded local flow, function-effect extraction, summary
propagation, and the cross-module overlay. The separation between local state,
summary state, and qualified cross-file state is justified by their different
lifecycles and certainty rules. The findings below target shared flow-plan
policy that is currently rebuilt by several consumers, plus concrete temporary
allocations and copies in hot bounded paths. They preserve correlated
alternatives, incomplete-versus-complete status, and fail-closed matching.

## Findings

### Flow-plan and lifecycle target selection

#### [x] READ-010 — Local, summary, and cross flow duplicate lifecycle target selection

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** DEDUPLICATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/planning.rs:326-349`; `glass-lint-core/src/analysis/flow/projector/transfer.rs:55-75`; `glass-lint-core/src/analysis/flow/projector/evidence.rs:68-117`; `glass-lint-core/src/analysis/flow/summary/sink.rs:217-269`; `glass-lint-core/src/analysis/flow/cross/sources.rs:230-283`; `glass-lint-core/src/analysis/flow/cross/propagation.rs:155-184`

The local projector, summary builder, and cross-flow source/sink paths each
rebuild the same target policy: prefer a global target, otherwise require a
rooted chain, look up the member target, and then apply argument constraints.
The copies differ in small syntax details (`then`, `then_some`, `flatten`, or
an owned `Vec`), so a future change to rooted eligibility or target precedence
can make local, summary, and cross-file flow disagree while all still compile.
The shared `BoundFlowPlan` currently owns the indexes but not the complete
selection operation.

**Recommendation:** Put source and sink candidate selection on the existing
flow-plan owner, accepting the narrow `CallShape`/argument view needed to
return the already indexed candidates or matching entries. Have local,
summary, and cross-flow consumers call that operation and retain only their
distinct state transitions, summary projection, or evidence behavior. Preserve
global-before-rooted precedence, the rooted-chain gate, argument predicates,
and the distinction between local completion and crossed evidence.

**Fix Applied:** Centralized global-before-rooted call-target selection on the
bound target index and exposed source/sink selection through `BoundFlowPlan`.
Local projection, summary construction, cross-flow propagation, and cross-flow
source discovery now share the same selector while retaining their existing
argument predicates, state transitions, and evidence behavior. Verified with
`make fmt && make ci`.

### Local projector state ownership

#### [ ] READ-011 — Source admission clones a newly built state batch before storage

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/flow/projector/transfer.rs:55-84`; `glass-lint-core/src/analysis/flow/projector/state.rs:466-520`

`match_source` constructs a `Vec<FlowState>` and immediately passes it by
shared reference to `FlowStateTable::admit_object`; the admission method then
clones every state into `insert_state_unchecked`, after which the original
batch is dropped. `insert_state_unchecked` already has a separate clone/log
requirement for reversible history, so the caller-to-admission clone is an
additional copy of the whole source batch on every matched source call.

**Recommendation:** Let the admission boundary consume the newly constructed
state batch, perform the same new-key capacity preflight, and retain only the
copy required by the mutation log's undo/redo semantics. Keep atomic rejection
of an over-limit batch, alias binding order, object identity sharing across
matched flows, and mutation-log exhaustion behavior unchanged; do not make
the state table's storage public to avoid the copy.

**Fix Applied:** None so far.

### Summary path representation

#### [ ] READ-012 — Summary path operations repeatedly materialize and rebuild segment vectors

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/flow/summary/store.rs:44-80,124-152,218-263`; `glass-lint-core/src/analysis/flow/summary/parameter.rs:94-128`

`SummaryPathWalk::segments` allocates and reverses a `Vec<PathSegment>` for
every walk. `visit_segments` immediately consumes that temporary vector,
`join` materializes a suffix only to append each segment into another path,
and `without_first` materializes a complete path and then rebuilds a path from
the empty root. These operations are used by parameter projection and summary
sink propagation, where the store already has parent/segment access for both
frozen and overlay representations.

**Recommendation:** Give `SummaryPathWalk` a streaming ordered visitor and
add owner-level prefix/suffix operations that traverse or append without an
intermediate segment vector; retain a materializing helper only for tests or
callers that truly need owned segments. Preserve frozen-versus-overlay ID
validation, linked-parent transitions, deterministic segment order, and
overlay-node exhaustion returning `None`.

**Fix Applied:** None so far.

### Cross-flow source propagation

#### [ ] READ-013 — Source propagation clones adjacency destinations for every pending item

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/flow/cross/sources.rs:129-142,285-339`

`FlowSources::propagate` obtains immutable destination references and collects
them into a new `Vec<SourceKey>` for every `(source key, candidate)` pending
item solely so it can mutate the source table while iterating. The adjacency
index is not changed during propagation, so this is a borrow-avoidance copy
whose cost is multiplied by candidate propagation through the graph.

**Recommendation:** Keep adjacency traversal and candidate insertion behind a
`FlowSources` operation that splits the immutable adjacency borrow from the
mutable source-table borrow, or otherwise iterates the stable destination
slice without materializing it. Preserve self-edge suppression, set-based
candidate deduplication, pending-frontier and total-retained bounds, and the
budget completion reason when a new candidate cannot be admitted.

**Fix Applied:** None so far.

### Fact-driven effect queries

#### [ ] READ-014 — Call-shape construction looks up the same fact twice

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Unnecessary Work
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:164-203`

`CallEffectRef::shape` calls `call_fact()` once to destructure the call payload
and then calls it again to obtain `effective_call_args`. Both accesses resolve
the same immutable fact by ID, and `shape` is used by local, summary, and
cross-flow passes. The second lookup adds repeated index work and obscures
that the two derived views must come from one call payload.

**Recommendation:** Bind the result of `call_fact()` once and derive both the
shape fields and effective arguments from that borrowed payload. Preserve the
unknown-fact `None` result, non-call rejection, unwrap-chain precedence, and
the existing borrowed lifetimes of `CallShape`.

**Fix Applied:** None so far.

## Systemic Themes

- `BoundFlowPlan` already centralizes physical target indexes and argument
  constraints, but consumers still reconstruct the semantic target-selection
  policy. The narrowest consolidation is a plan-owned selection API, not a
  shared mutable flow state or provider-specific abstraction.
- Bounded flow uses deliberate copies where rollback, path correlation, or
  evidence ownership requires them. The reported copies are the temporary
  source batch, adjacency destination vector, and summary segment vectors
  that have no independent lifecycle.
- Local and cross-file certainty must remain distinct. Consolidating target
  lookup must not merge local object-state transitions with cross-context
  evidence or allow an incomplete alternative to establish a definite match.

## Open Questions

- The plan-owned selector should return borrowed candidate slices or an
  iterator; callers may collect only at the existing state-borrow boundary.
  It must not create a new per-call vector merely to bridge lifetimes.
- The state table must consume the temporary batch while its inverse log keeps
  the existing rollback copies. Direct test insertion remains a separate
  single-state helper and does not justify keeping the batch borrowed.

## Coverage

Reviewed the chunk-03 structure entries and their implementation/test support:

- `analysis/flow/{mod,matcher,planning}.rs`
- `analysis/flow/effect/{mod,tests}.rs`
- `analysis/flow/cross/{mod,evidence,graph,propagation,sources,state,worklist}.rs`
- `analysis/flow/projector/{mod,control,evidence,history,loops,state,transfer,tests}.rs`
- `analysis/flow/summary/{mod,parameter,sink,store,summaries}.rs`

No source, test, configuration, dependency, or other documentation files were
changed by this audit.
