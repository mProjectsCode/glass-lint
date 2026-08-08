# Codebase Readability Audit

## Summary

Chunk 3 owns bounded local object flow, function effects and summaries, the
cross-call overlay, and flow evidence emission. The projector’s path
correlation and the immutable fact boundary are strong designs, but flow
identity, completion policy, and evidence construction are each represented
by parallel APIs at the local/cross boundary. Those parallel paths make
correctness depend on matching conventions instead of one flow-owned
contract.

## Findings

### Flow identity and plan binding

#### [x] READ-008 — Give lifecycle roots one canonical flow identity

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:133-154`; `glass-lint-core/src/analysis/project/projection.rs:223-250, 282-295`; `glass-lint-core/src/analysis/flow/projector/mod.rs:133-155`; `glass-lint-core/src/analysis/flow/planning.rs:209-255`; `glass-lint-core/src/analysis/flow/cross/mod.rs:289-303`

The same lifecycle flow ID is reconstructed independently by local and
cross-call execution. Local planning stores the physical-root index in
`FlowId`, while `cross::collect_flows` recomputes a compact counter that
increments only for lifecycle roots; `BoundFlowPlan::new` then accepts a
positional `(RuleIndex, usize, &CompiledObjectFlow)` tuple. Current compiler
validation requires a lifecycle root to be top-level, which usually masks the
split, but no flow-owned type proves that the two indexing conventions remain
equal. Because the ID keys flow state, source candidates, caches, and evidence,
an ordering or root-shape change can silently correlate local and cross data
with different flows.

**Recommendation:** Have the physical-plan boundary produce one typed
`BoundLifecycleRoot`/`FlowIndex` entry for each lifecycle root and pass those
entries to both local and cross planning. Make `BoundFlowPlan::new` consume
that domain collection instead of a positional tuple, then delete the
cross-call re-enumeration and `FlowProjectionRule::as_bound_flow` tuple
conversion. Preserve deterministic root sorting/deduplication, the rule
index, and the top-level lifecycle invariant, while keeping raw physical-root
indices private to the compiler plan.

**Fix Applied:** Added `BoundLifecycleRoot` at the physical-plan boundary and
passed the same typed root collection to local and cross-module projection.
Removed cross-call re-enumeration and positional tuple binding so both paths
share one `FlowId` assignment. Verified with `make fmt && make ci`.

### Completion and incomplete-evidence contract

#### [x] READ-009 — Preserve independent possible witnesses when flow is incomplete

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/mod.rs:19-68`; `glass-lint-core/src/analysis/flow/summary/summaries.rs:57-65, 82-105`; `glass-lint-core/src/analysis/flow/cross/mod.rs:208-231`; `glass-lint-core/src/analysis/flow/projector/state.rs:640-733`; `glass-lint-core/src/analysis/project/projection.rs:458-589`

Flow has a detailed `FlowCompletion` reason set, but the projection boundary
reduces local and cross outcomes to a boolean `ProjectionCompletion` and an
aggregate operation count. More importantly, summary finalization clears all
function sinks when any summary budget is incomplete, cross projection clears
all module evidence when any source/worklist/step limit is hit, while local
evidence retains complete occurrences and marks only rejected keys as
truncated. These are different answers to the same contract and can erase an
independent complete witness that should remain a `Possible` result while
preventing a `Definite` result.

**Recommendation:** Make a flow-owned outcome carry phase reasons and
candidate-level completeness into the evidence sink, rather than converting
them to a project-wide boolean before evidence is merged. Prune or downgrade
only states whose source, path, or propagation alternative is incomplete;
retain an independent complete witness as possible, and never let any
incomplete alternative establish definite coverage. Keep deterministic reason
aggregation and explicit distinctions between effect, summary, local-path,
cross-context, evidence, and trace exhaustion.

**Fix Applied:** Flow summaries now retain sorted sinks after incomplete
propagation, and local/cross evidence sinks retain emitted witnesses instead
of clearing them when a bounded phase is incomplete. The flow output boundary
downgrades retained evidence to `Possible` whenever completion carries an
exhaustion reason, preserving independent witnesses without allowing
incomplete work to establish `Definite`. Added local object-limit and cross
evidence regressions. Verified with `cargo test -p glass-lint-core
analysis::flow` and `make fmt && make ci`.

### Flow evidence aggregation

#### [x] READ-010 — Share one bounded flow-evidence accumulator

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:640-733`; `glass-lint-core/src/analysis/flow/projector/evidence.rs:209-261`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:39-145, 147-254`

Local flow uses `FlowEvidence` with `ReportEvidenceKey`, per-key and total
reservations, rollback, and event truncation. Cross flow separately implements
`ModuleEvidence`/`RuleEvidence` with a different `EvidenceKey`, nonmatching-set
promotion to `Possible`, occurrence deduplication, and final conversion into a
`RuleEvidenceTable`; it has no equivalent admission operation at the record
site. Both are flow-specific sinks that merge certainty and traces, but their
limits, keys, and duplicate policies can evolve independently, so local and
cross findings do not share one bounded output contract.

**Disposition:** Revalidated after the evidence-chain and classification
boundaries were centralized. Certainty promotion is already owned by
`ClassificationEvidence`, and trace-node interning is shared by `TraceArena`.
Local bounded admission and cross-module nonmatching/occurrence policies are
not identical: combining them would change per-key limits, incomplete
alternative handling, or deterministic output. The two evidence stores are
therefore intentionally retained as separate owners; no actionable
deduplication remains for this finding.

**Fix Applied:** Closed as non-actionable after revalidation; no source change
was appropriate. The latest source gate remains `make fmt && make ci`.

### Evidence trace construction

#### [x] READ-011 — Centralize lifecycle trace-chain assembly

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/evidence.rs:264-294`; `glass-lint-core/src/analysis/flow/cross/evidence.rs:174-197`; semantic prior-sink accessors `glass-lint-core/src/analysis/model/flow.rs:585-587` and `glass-lint-core/src/analysis/flow/cross/state.rs:182-185`

Local and cross flow independently assemble the same source → requirements →
prior sinks → terminal sink chain before calling `TraceArena::intern_chain`.
The implementations already disagree about the semantic role of prior sinks:
local emits them as `Requirement`, while cross emits them as `Sink` and has a
regression test asserting that choice. The duplicated assembly therefore
leaks evidence-role policy into two projectors and can produce different
report traces for the same lifecycle relation.

**Recommendation:** Give the flow evidence/trace boundary one helper that
accepts typed source, requirement, prior-sink, and terminal-sink events and
performs qualification and ordering once. Have local and cross callers supply
only their module context and event sources, then delete both private chain
builders. Preserve source-to-sink order, the intended prior-sink role,
cross-module qualification, deterministic requirement order, and `None` on
trace-arena exhaustion without returning partial chains.

**Fix Applied:** Centralized lifecycle source/requirement/prior-sink/terminal
chain assembly in `TraceArena` while preserving the distinct local
`Requirement` and cross-module `Sink` prior-sink roles. Verified with
`make fmt && make ci`.

## Systemic Themes

- `FlowId`, completion status, evidence keys, and trace heads cross several
  flow phases, but their contracts are reconstructed at each local/cross
  boundary. These should be owned by flow-domain types rather than raw tuples,
  booleans, and separate map-based sinks.
- The local projector correctly keeps aliases, lifecycle state, checkpoints,
  and path completeness correlated. Any shared evidence or completion owner
  must preserve that correlation and must not combine incompatible paths.
- Bounded analysis needs two separate guarantees: incomplete work cannot make
  a definite finding, and an independent complete witness must remain usable
  as a possible finding. Output ownership should encode both guarantees.

## Decisions

- Keep one `FlowIndex` per lifecycle root even though current validation emits
  lifecycle roots only at the top level. The identity should be assigned at
  the physical-plan boundary and carried into both local and cross-call flow;
  it should not depend on whether a future normalization shape happens to
  preserve that restriction.
- Prior sinks are not one universal evidence role today. The public schema
  supports both `Requirement` and `Sink`, and the existing cross-flow contract
  deliberately reports prior sinks as `Sink` while local flow reports them as
  `Requirement`. READ-011 should centralize chain ordering and arena admission
  while accepting the role as typed input; it must not force the two policies
  into one role.

## Coverage

Reviewed only Chunk 3, “Flow analysis,” from `CODEBASE_STRUCTURE_CORE.md`,
including function effects, bound flow plans, local object projection and
control state, summaries and path overlays, cross-call graph/worklists/source
propagation, completion outcomes, flow evidence, and trace emission. Existing
Chunk 1 and Chunk 2 audit history was used to continue IDs at READ-008. No
source, test, configuration, dependency, or other documentation files were
changed; this chunk audit file is the only new artifact.
