# Codebase Readability Audit

## Summary

Chunk 03 (`analysis::flow`) has a deliberate architecture: local projection
owns one-file object state, effects and summaries expose reusable function
relations, and cross-flow propagation overlays qualified calls without
re-traversing syntax. The checkpoint, canonical-state, evidence, and bounded
worklist types generally preserve those responsibilities well.

The main opportunities are around admission and transport boundaries. Summary
capacity is checked after mutation, local projection copies immutable data in
hot paths, effect records repeat a large initialization shape, and one cross
flow helper merely adapts an already-generic planner function. These findings
keep the existing bounded and fail-closed behavior as explicit guardrails.

## Findings

### [analysis/flow/summary]

#### [ ] READ-008 — Make summary sink admission atomic with insertion

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/summary/sink.rs:56-85,143-199,217-262`; `glass-lint-core/src/analysis/flow/summary/summaries.rs:28-54,133-166,168-218`

`SinkSet` owns the retained `FunctionSinkSummary` values, but it has no
capacity boundary. `FunctionSummary::collect_sinks_for_call` inserts all
candidate sinks through `add_sinks` before `FunctionSummaries::collect_direct_sinks`
calls `SummarySinkBudget::admit`; propagation has the same order when
`caller_summary.add_sink(proj)` is followed by `sink_budget.admit`. If the
global sink capacity or operation budget rejects the admission, the newly
inserted sink values remain in the summary even though the phase is marked
incomplete. The budget owner therefore cannot make the storage mutation and
the completion transition one bounded operation.

**Recommendation:** Move admission into the summary-collection owner as one
operation that computes the novel entries, checks the shared budget/capacity,
and only then mutates `SinkSet`, or give `SinkSet` a bounded insertion API
that accepts the shared admission token. Delete the post-hoc `InsertOutcome` /
`SummarySinkBudget::admit` split once callers use the atomic operation. Preserve
deduplication, deterministic final sorting, monotonic propagation, and the
fail-closed rule that an exhausted summary cannot establish definite flow.
Add tests that exceed capacity during both direct-sink collection and
callee-to-caller propagation and assert that retained summaries stay within
the advertised bound.

**Fix Applied:** `SummarySinkBudget::admit_sinks` now computes novel sink
entries, checks the global capacity and operation budget, and mutates the
owning `FunctionSummary` only after admission succeeds. Direct collection and
callee-to-caller propagation share this operation; the old post-hoc
`InsertOutcome` split is gone. Verified with
`cargo test -p glass-lint-core analysis::flow::summary --lib`.

### [analysis/flow/projector/evidence.rs, analysis/flow/projector]

#### [ ] READ-009 — Remove immutable plan and summary copies from projection hot paths

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/evidence.rs:68-114,116-168`; `glass-lint-core/src/analysis/flow/planning.rs:94-134,209-312`

`ObjectFlowProjector::record_sinks` converts the borrowed sink-candidate slice
into a new `Vec<BoundSink>` on every matching call via `map(<[_]>::to_vec)`,
although the candidates are only read. `record_helper_sink` then clones the
entire `FunctionSummary` and copies the stream’s parameter slice before
collecting `(FlowId, ValueId)` results; the clone exists only to keep immutable
borrows separate from the later emission loop. These copies add work and make
the caller look as though it owns a snapshot of the plan/summary when the
actual owner remains `BoundFlowPlan`/`FunctionSummaries`.

**Recommendation:** Keep the candidate slice borrowed, and scope the borrowed
summary and parameter bindings only through the collection of lightweight
ready values; emit after that borrow ends. Delete the `to_vec` candidate copy,
`FunctionSummary::clone`, and parameter copy while retaining the existing
borrow-safe phase split. Guard candidate ordering, helper path projection,
duplicate flow suppression, and emission only after all matching values have
been collected.

**Fix Applied:** None so far.

### [analysis/flow/effect]

#### [ ] READ-010 — Centralize repeated `FunctionEffect` construction

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:264-273,565-632`

`FunctionEffectsBuilder` constructs the six-field `FunctionEffect` record
three times: for the synthetic function-zero entry, for a function whose
parameters are unavailable, and for a normal parameterized function. Each
literal repeats empty call/use/return vectors and value-root/index maps while
only the identity, invalid flag, and parameter-derived maps differ. A new
effect field or initialization invariant must therefore be updated in three
places, and the construction policy is owned by the builder rather than by
`FunctionEffect`.

**Recommendation:** Add narrow constructors on `FunctionEffect` for an empty
effect, an invalid effect, and a parameterized effect, with one private base
initializer owning the common collections. Replace the three literals and
delete their repeated storage setup. Preserve the synthetic function-zero
entry, invalid-summary behavior when parameters are missing, parameter-root
mapping, and all effect-budget accounting in the builder.

**Fix Applied:** `FunctionEffect::empty`, `invalid`, and `with_parameters`
now own the shared effect-record initialization. The synthetic function-zero,
missing-parameter, and normal parameterized paths use those constructors, so
their collection storage cannot drift. Verified with
`cargo test -p glass-lint-core analysis::flow::effect --lib`.

### [analysis/flow/cross/sources.rs, analysis/flow/planning.rs]

#### [ ] READ-011 — Delete the one-call source-index adapter

- **Severity:** Low
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Conversion
- **Location:** `glass-lint-core/src/analysis/flow/cross/sources.rs:197-207,223-246`; `glass-lint-core/src/analysis/flow/planning.rs:155-173,302-307`

`cross::sources::build_source_index` only converts a `HashMap` into an
iterator and supplies `BoundSource::new` to the generic
`planning::build_source_index`. It is private and has one production caller,
so it adds a second name and a second apparent source-index owner without
adding validation or a different indexing policy. The same generic builder is
already called directly by `BoundFlowPlan::new` for the plan’s source index.

**Recommendation:** Inline the iterator and closure at
`FlowSources::collect_candidates` and delete the private adapter. Keep the
per-module `NameTable` binding, deterministic target normalization, and the
source-candidate argument matching behavior; the cleanup should remove only
the forwarding layer, not consolidate artifact-local name paths across
modules.

**Fix Applied:** None so far.

## Systemic Themes

- Boundedness is a semantic invariant, not just a counter. Admission should be
  adjacent to the collection it limits, with mutation and exhaustion reported
  atomically.
- The flow layer has good domain wrappers for paths, states, worklists, and
  evidence. The projector’s separate path-machine, live-state, and run-outcome
  owners are intentionally retained as an architectural question rather than a
  finding until a smaller deletion target is demonstrated.
- The local/cross split is architecturally sound; the cross layer should reuse
  generic planning operations directly and keep provider-neutral artifact-local
  path identity explicit.

## Review Resolutions

- Keep summary sink capacity global: `SummarySinkBudget` is the current owner
  of that bound, so READ-008 must make its admission atomic without inventing
  a second per-function limit.
- Keep `ProjectionPathMachine::binding_slots` outside path checkpoints. It is
  a stable lexical-slot representative; checkpointing it would conflate stable
  binding identity with path-local object identity.
- Do not introduce a broad coordinated projector owner. READ-009 should remove
  only the demonstrable copies while retaining the existing `TraceArena` and
  evidence ownership boundaries.

## Coverage

Reviewed Chunk 03: `analysis::flow` completion and limits; planning and bound
source/sink indexes; local object-flow projection, control frames, loop fixed
points, state tables, mutation history, transfer, and evidence; function
effects; cross-module call graphs, sources, propagation, worklists, and
evidence; and function summaries, parameter projection, and overlay path
storage. Traced representative callers across local collection and cross-flow
collection. Read the root/core architecture, testing/contributing guidance,
the complete readability-audit skill instructions, and existing audits 001–002.
No source or test files were changed.
