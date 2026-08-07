# Codebase Readability Audit — Chunk 8

## Summary

Chunk 8 owns the provider-neutral effect model, bound flow plans, and bounded
local object-flow projector. The design correctly keeps AST resolution out of
execution, preserves correlated alternatives, and makes loop convergence and
resource limits explicit. The main architectural risks are at the seams
between raw fact views and planned matching, between the projector and its
frontier state, and between bounded admission/evidence state and the final
projection outcome.

## Findings

### Call-effect view boundary

#### [x] READ-036 — Make `CallEffectRef` a closed call-shape boundary

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:99-103,141-160,163-295`; `glass-lint-core/src/analysis/flow/planning.rs:28-82`; callers in `flow/projector/transfer.rs:20-83` and `flow/cross/sources.rs:265-289`

`CallEffectRef` exposes its backing `FactStream` and `FactId` to every module
inside `analysis`, has both `EffectCall::as_ref(stream)` and
`FactStream::call_effect(event)` construction paths, and combines raw fact
decoding with compiled-flow matching through `matches_target` and
`matches_source`. Its `chain` and `chain_owned` methods also repeat the same
unwrap/rooted/syntactic path precedence, while `FlowMatchView` and
`BoundFlowPlan` independently own target and argument matching. Callers must
choose which view and which path fallback to use, and an internal caller can
construct a view whose event and stream do not belong to its intended effect
artifact.

**Recommendation:** Keep the view fields private and expose one validated
factory per owning source, then derive a small canonical call-shape view that
owns fact lookup and the single path-precedence operation. Move compiled
source/target matching to `FlowMatchView`/`BoundFlowPlan`, deleting the
forwarding matcher methods and duplicated chain extraction from
`CallEffectRef`. Preserve unknown-event fail-closed behavior, unwrap-chain
precedence, rooted-versus-syntactic distinction, static name resolution, and
the separation between artifact-local facts and compiled rule plans.

**Fix Applied:** Made `FactStream::call_effect` the sole validated factory
and made `CallEffectRef` fields private. Added a canonical `CallShape` that
owns call-fact decoding, unwrap/effective-argument selection, chain
precedence, rootedness, and call identity metadata. Removed compiled
source/target matching from the effect view and performed it through
`FlowMatchView` at the planning, summary, and cross-flow callers.

### Prepared argument data

#### [x] READ-037 — Let `ArgumentData` own prepared-versus-arena fallback

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/matcher.rs:58-107,110-128`; implementations at `flow/matcher.rs:130-173`

`ArgumentMatcher::matches` repeatedly decides whether to use an overlay or
prepared object/path/string first and then fall back to the `ValueTable`:
the object-key branch, rooted-expression branch, and object-property-value
branch each rebuild part of that policy. `ArgumentData` exposes the raw and
prepared accessors separately, so every new matcher kind must remember the
same precedence and fail-closed behavior. The optimization boundary is
therefore maintained in the matcher’s variant dispatch instead of by the
argument view that owns the prepared data.

**Recommendation:** Add narrow canonical accessors on `ArgumentData` (or a
private `FlowArgumentView`) for resolved static string, static object, and
rooted chain, each applying prepared/overlay-before-arena fallback once.
Have `ArgumentMatcher` consume those semantic values and delete the repeated
`prepared_*().or_else(...)` branches. Preserve artifact-local `ValueId`
resolution, overlay precedence, dynamic-value rejection, rooted-chain
matching, and the distinction between an unavailable value and a successful
empty match.

**Fix Applied:** Added canonical `ArgumentData` accessors for static strings,
static objects, and rooted chains. Each accessor applies prepared or overlay
data before frozen-arena fallback, and all matcher variants—including object
property values—now consume those accessors instead of rebuilding precedence
locally. Dynamic and unavailable values remain fail-closed.

### Projector frontier lifecycle

#### [x] READ-038 — Give `PathFrontier` ownership of path batches and transfer

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:180-342,452-480,603-623`; `flow/projector/control.rs:41-256`

`PathFrontier` has generation and active-path state, but its owning
projector and the control module still read and mutate `frontier.paths`
directly for branch, loop, switch, try, and function transitions. The
projector also repeats the restore/charge/reachability/outgoing-path loop in
`transfer_paths` and `transfer_paths_without_finalization`; only the active
batch selection and pending-finalization phases differ. The invariant that a
`PathToken` belongs to the current generation and that every restored
environment is paired with the correct frontier lifecycle is consequently
spread across raw vector operations and two transfer loops.

**Recommendation:** Make `PathFrontier` expose operation-oriented snapshots,
replacement, append, and active-batch transfer methods, and remove direct
`paths` access from `control.rs`. Extract the shared per-environment
restore/charge/reachability operation into the projector/frontier owner while
keeping ordinary transfer and loop/function replay as explicit phases rather
than adding a boolean mode. Preserve generation invalidation, path-local
correlation, pending-state finalization timing, deterministic ordering, and
fail-closed behavior when restoration or the operation budget fails.

**Fix Applied:** Encapsulated frontier path storage behind snapshot, replace,
append, take, count, and presence operations, removing direct vector access
from control transitions and loop replay. Centralized per-environment
operation charging, restoration, reachability, and failure classification in
the projector’s `restore_path` helper while keeping ordinary transfer and
function replay as separate phases.

### Semantic path admission

#### [x] READ-039 — Share semantic-path admission between joins and loops

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:658-691`; `flow/projector/loops.rs:89-145,166-200`

`ObjectFlowProjector::join_paths` and `LoopFixedPoint::admit_into` both
charge the projection budget, restore a `FlowEnvironment`, canonicalize the
result with `FlowStateTable::semantic_snapshot`, and classify an admission as
complete, duplicate, or incomplete. They use different local collections
(`Vec` with linear snapshot comparison for joins and `BTreeSet` for loops),
but the shared resource and uncertainty transitions are duplicated. Adding a
new canonical state component or changing restore-failure handling requires
updating both paths, risking disagreement between branch joins and loop
fixed points.

**Recommendation:** Put the common restore/charge/snapshot admission
operation on the local projector or a focused private path-admission owner,
returning a typed admission result while leaving each caller’s collection and
deduplication policy intact. Delete the repeated budget, restore, snapshot,
and incomplete-flag choreography after migration. Preserve loop replay versus
join semantics, alternative limits, deterministic collection order, semantic
object-ID normalization, and the rule that failed or exhausted alternatives
cannot establish a definite witness.

**Fix Applied:** Added `ObjectFlowProjector::admit_path` as the shared owner
of operation charging, environment restoration, semantic snapshot admission,
and incomplete transitions. Joins retain source-order vectors while loops
retain replay and exit sets; both now consume the same typed admission result.

### Local flow evidence sink

#### [x] READ-040 — Make bounded evidence reservation and recording one operation

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:510-586`; call sites in `flow/projector/evidence.rs:199-256` and projector finalization at `flow/projector/mod.rs:787-805`

`FlowEvidence` reserves a `(rule, flow, object, event)` key in one mutable
map, then the caller builds a trace and separately calls `record` into an
externally owned `RuleEvidenceTable`; truncation is applied later by a third
`mark_truncated` pass. The reservation and report storage can therefore be
temporarily inconsistent, and both `record` and `mark_truncated` use
`expect` because catalog-capacity correctness is maintained outside the
evidence type. The local projector’s bounded admission, trace construction,
report insertion, and truncation policy are split across the state owner,
emission caller, and finalizer.

**Recommendation:** Give a private local evidence sink ownership of the
validated catalog-capacity boundary and expose one operation that admits a
key, builds or receives its trace, records the occurrence, and marks the
appropriate truncation state. Keep the externally supplied report table only
as the final merge target, or wrap it in a capacity-validated sink so the
`expect` paths disappear. Preserve per-key and total limits, trace-arena
exhaustion, deterministic evidence order, event-level truncation, and the
fact that an incomplete flow cannot be upgraded by evidence bookkeeping.

**Fix Applied:** Replaced separate evidence reservation and report recording
with `FlowEvidence::record_if_admitted`. The operation owns bounded admission,
catalog insertion, rollback, and truncation state; trace construction now
precedes admission, so counters cannot represent an unrecorded occurrence.

### Local projection completion

#### [x] READ-041 — Give local projection exhaustion one completion owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/mod.rs:51-66,161-177,356-382,787-805`; `flow/projector/state.rs:462-472,583-585`; `analysis/project/projection.rs:314-385`

`LocalFlowProjectionOutcome::exhausted` is assembled in `into_outcome` from
the summary flag, object-allocation rejection, state-limit rejection,
evidence-limit rejection, mutation-log exhaustion, alternative completeness,
and trace-arena exhaustion. Those conditions live on different state types,
and the project layer then maps the single boolean into both
`ProjectionStatus::local_exhausted` and `ProjectionStatus::flow_exhausted`.
The status/counter contract is thus maintained by a list of parallel flags
and a second translation layer; a new bounded resource can be added to one
side without becoming visible in the other.

**Recommendation:** Introduce a local projection completion value that owns
the terminal resource checks and produces both the public outcome and the
project-layer status contribution, while keeping counters separate from
completion state. Have each bounded owner report a typed exhaustion reason to
that value and delete the repeated boolean aggregation/mapping. Preserve
local-versus-cross status distinctions, all existing metrics, fail-closed
evidence clearing/interpretation, and the rule that an independent complete
possible witness remains possible even when another alternative is
incomplete.

**Fix Applied:** Replaced the aggregate `exhausted` flag with a typed local
projection completion value. It owns the terminal checks for summary,
object, state, evidence, mutation, alternatives, and trace exhaustion, while
the project layer reads that value once to update both local and flow status.
Projection counters and fail-closed behavior remain unchanged.

## Systemic Themes

- Effects and plans are provider-neutral and fact-driven, but the call-view
  boundary currently crosses extraction, path normalization, and execution
  matching responsibilities.
- The projector has deliberate bounded correlated alternatives; its path
  frontier, semantic admission, evidence reservation, and completion state
  should each have one clear owner without collapsing distinct loop, branch,
  replay, or report lifecycles.
- Any refactor must preserve strict path-local identity, deterministic order,
  explicit `Possible` versus `Definite` certainty, and fail-closed behavior
  for unsupported, ambiguous, restored-failed, or exhausted alternatives.

## Decisions

- `chain_owned` is needed only for the projector’s alias/callee fallback. The
  canonical call-shape view should keep that conversion opt-in instead of
  making every effect consumer carry a `NameTable`.
- Local and cross-flow projection intentionally share one
  `RuleEvidenceTable` per project projection. A local evidence sink should
  wrap the shared table and own reservation/truncation, not allocate a second
  report matrix.
- `ProjectionStatus::flow_exhausted` intentionally includes local flow
  exhaustion, while `local_exhausted` preserves the narrower diagnostic. A
  completion owner must emit both meanings rather than changing the public
  status mapping.

## Coverage

Reviewed all types listed in Chunk 8 of `CODEBASE_STRUCTURE_CORE.md`:

- Effects and matching: `CallEffectRef`, `EffectArgument`, `EffectCall`,
  `EffectCallId`, `EffectUse`, `FunctionEffect`, `FunctionEffects`,
  `FunctionEffectsBuilder`, `ParameterRef`, `ReturnProjection`, and
  `ArgumentData`.
- Planning: `BoundFlowPlan`, `BoundLifecycleCallTarget`, `BoundSource`,
  `BoundTargetIndex`, `FlowMatchView`, and `PropertyRequirementMatch`.
- Projection: `ActivePaths`, `AlternativeCompleteness`, `EmissionMode`,
  `LocalFlowProjectionOutcome`, `ObjectFlowProjector`,
  `ObjectFlowProjectorInput`, `PathFrontier`, `PathToken`, `PendingFlowKey`,
  `PendingFlowStateFinal`, `PendingFlowStates`, `PendingState`,
  `ProjectionRunState`, `Checkpoint`, `InverseDelta`, `MutationLog`,
  `ReportEvidenceKey`, `LoopAdmission`, `LoopFixedPoint`,
  `LoopFixedPointOutcome`, `AbruptExit`, `AliasTable`, `CanonicalAlias`,
  `CanonicalFlowState`, `CanonicalObjectId`, `CanonicalRequirementState`,
  `CanonicalSinkState`, `ControlFrame`, `ControlStack`, `FlowEnvironment`,
  `FlowEvidence`, `FlowSemanticSnapshot`, `FlowStateTable`,
  `ObjectRefCounts`, and `PropertyWriteUpdate`.

Representative callers in project projection, local transfer, control-frame
handling, loop convergence, effect extraction, and evidence finalization were
traced. Chunk 2 findings for effect-builder stream ownership, mutation-log
replay, state-limit admission, and bounded worklists were not duplicated.
