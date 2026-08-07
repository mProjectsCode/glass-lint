# Codebase Readability Audit — Chunk 2

## Summary

Chunk 2 owns bounded local and cross-call flow over the immutable fact stream.
The design correctly separates provider-neutral effects, pre-bound plans,
local path projection, summary propagation, and qualified cross-module flow.
The main maintainability risk is that several important bounds and reversible
state invariants are enforced by neighboring callers rather than by the type
that owns the state, while some construction APIs repeatedly pass the same
stream and expose multiple lifecycle modes.

## Findings

### Effect extraction ownership

#### [ ] READ-005 — Make the effect builder own its fact-stream context

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/flow/effect/mod.rs:576-706`

`FunctionEffectsBuilder::new` receives a `FactStream` to size and enable the
builder, but does not retain it; every `consume` call receives the same stream
again, and `FunctionEffect::parameter_for` accepts a `_stream` argument that it
does not use. `FunctionEffects::collect` consequently carries the stream
through a loop and through several effect-recording methods even though one
immutable stream defines the entire builder lifetime. This obscures the
single-stream ownership contract and makes the internal API easier to call
with an unrelated stream.

**Recommendation:** Make the builder explicitly borrow the one frozen stream
for its construction lifetime and reduce `consume` and effect-recording calls
to the fact-specific inputs they need; remove the unused stream parameter from
`parameter_for`. Delete the repeated stream plumbing in
`FunctionEffects::collect` and the forwarding calls once the owner is
established. Preserve the shared fact pass, artifact-local identities,
invalid-effect fail-closed behavior, budget accounting, and the rule-neutral
effect model.

**Fix Applied:** None so far.

### Reversible local state

#### [ ] READ-006 — Keep mutation-log replay behind `FlowStateTable`

- **Severity:** High
- **Fix Complexity:** High
- **Theme:** ENCAPSULATE
- **Category:** Architecture
- **Location:** `glass-lint-core/src/analysis/flow/projector/state.rs:71-83,488-506`; `glass-lint-core/src/analysis/flow/projector/history.rs:33-177`

`FlowStateTable` owns aliases, lifecycle states, state limits, and the
mutation log, but `MutationLog::transition` accepts a mutable `AliasTable` and
a raw `BTreeMap<FlowStateKey, FlowState>`. The free `apply_inverse` and
`apply_forward` functions then mutate those representations directly, while
ordinary writes go through `FlowStateTable` methods. The rollback invariant is
therefore implemented in two layers: adding a new state component requires
updating both normal mutation logging and both replay directions, and the raw
state map is visible outside its owner.

**Recommendation:** Make checkpoint restoration a `FlowStateTable` operation
whose history implementation is private to that owner; if the log remains a
separate type, expose only a domain-level transition callback or opaque delta
application operation rather than raw tables. Delete the raw map arguments and
the representation-level replay entry points from the projector boundary.
Preserve alias reference-count maintenance, reversible branch and loop
restoration, deterministic semantic snapshots, and the distinction between a
restored state and an exhausted/failed restoration.

**Fix Applied:** None so far.

### State-capacity policy

#### [ ] READ-007 — Centralize batched state-limit admission

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Encapsulation
- **Location:** `glass-lint-core/src/analysis/flow/projector/transfer.rs:18-47`; `glass-lint-core/src/analysis/flow/projector/state.rs:373-393,462-472`

`ObjectFlowProjector::assign` predicts whether a batch of matched source
states will fit, manually marks `state_limit_rejected`, binds the target, and
then ignores the boolean result from each `FlowStateTable::insert_state`.
`FlowStateTable::insert_state` independently applies the state limit and
records insertion/update deltas. The same capacity invariant consequently has
two owners and two failure paths, and a future change to replacement,
deduplication, or accounting can make the preflight disagree with insertion.

**Recommendation:** Add one domain operation on `FlowStateTable` for admitting
the object plus its state batch, including the capacity decision, rejection
status, alias binding, and mutation-log updates. Remove the preflight and
ignored per-state results from `assign`; callers should receive one explicit
admission outcome. Preserve updates to existing keys, object allocation
limits, rollback logging, and fail-closed incompleteness when the batch cannot
fit.

**Fix Applied:** None so far.

### Cross-flow worklists

#### [ ] READ-008 — Consolidate bounded deduplicating FIFO admission

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** DEDUPLICATE
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/flow/cross/worklist.rs:24-67`; `glass-lint-core/src/analysis/flow/cross/sources.rs:76-221,306-349`

`ContextWorklist` and `FlowSources::propagate` independently implement the
same bounded FIFO pattern: a `VecDeque`, a `BTreeSet` of retained entries,
duplicate detection, a `Full` admission result, and an exhaustion condition.
The source path has a separate `PropagationAdmission` and
`admit_propagation` helper, while the context path has `ContextAdmission` and
`ContextWorklist::push`. This duplicates the most important boundedness logic
in the cross-flow layer and makes future changes to retained-versus-pending
limits easy to apply to only one traversal.

**Recommendation:** Introduce one private bounded deduplicating queue primitive
at the narrowest shared cross-flow owner, then keep thin domain wrappers for
contexts and source-candidate propagation. Delete the duplicated admission
enum/helper and queue/set checks while retaining each domain’s key type,
exhaustion semantics, deterministic ordering, and total-retained versus
frontier limits. Do not merge this with the summary `BTreeSet` worklist or the
local projector’s correlated path frontier; their convergence and certainty
semantics are distinct.

**Fix Applied:** None so far.

### Function summary compatibility

#### [ ] READ-009 — Separate invocation arity from parameter-path projection

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/flow/summary/sink.rs:199-237`; `glass-lint-core/src/analysis/flow/summary/parameter.rs:19-73`

`FunctionSummary::is_invocation_compatible` combines spread rejection,
ordinary/rest arity checks, unknown-argument rejection, missing/default
parameter handling, and nested parameter-path projection. The related
`ParameterBinding::project_argument_at` then repeats the rest/default/path
decision tree for the actual projection. These are different levels of the
summary contract, so the current API makes it difficult to see whether a
failure is call-shape incompatibility or an unproven value/path transfer.

**Recommendation:** Let `FunctionSignature` own call-shape admission and let
`ParameterBinding` or `FunctionSummary` own a named, fail-closed parameter
projection check; have summary construction compose those results. Delete the
mixed branching from `is_invocation_compatible` and reuse the canonical path
projection operation rather than maintaining parallel rest/default logic.
Preserve rejection of spread and unknown values, required-parameter/default
semantics, rest-parameter paths, dynamic or invalid paths, and the rule that
unsupported projection cannot establish a flow witness.

**Fix Applied:** None so far.

## Systemic Themes

- Local flow has strong domain types and explicit bounded outcomes, but state
  admission, mutation replay, and result completeness still span multiple
  owners and flags.
- Cross-flow uses deterministic ordered collections consistently; its repeated
  bounded-worklist mechanics should be consolidated without conflating source
  propagation, context traversal, summary convergence, or path correlation.
- Flow APIs must remain provider-neutral and fact-driven. Refactors must not
  reintroduce AST traversal, widen unknown values, combine incompatible paths,
  or turn exhausted analysis into a definite finding.

## Decisions

- `FunctionEffectsBuilder` is an artifact-local builder over one frozen stream;
  no multi-stream contract exists in the workspace. Make that stream lifetime
  explicit and do not add a speculative stream collection abstraction.
- The shared queue primitive must retain separate `max_retained` and
  `max_pending` policies. Context worklists currently bound total retained
  contexts, while source propagation also bounds its pending frontier; the
  refactor may share admission mechanics but not collapse those budgets.

## Coverage

Reviewed all modules listed in Chunk 2 of `CODEBASE_STRUCTURE_CORE.md`:

- `analysis::flow`, `cross`, `cross::evidence`, `cross::graph`,
  `cross::propagation`, `cross::sources`, `cross::state`,
  `cross::worklist`, `effect`, `matcher`, `planning`, `projector`,
  `projector::control`, `projector::evidence`, `projector::history`,
  `projector::loops`, `projector::state`, `projector::transfer`, `summary`,
  `summary::parameter`, `summary::sink`, `summary::store`, and
  `summary::summaries`.

Representative callers in local lowering, project linking, and the flow unit
tests were checked for lifecycle, ownership, budget, and certainty behavior.
