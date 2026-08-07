# Codebase Readability Audit — Chunk 11

## Summary

Chunk 11 owns the retained fact, flow-state, lifecycle-evidence, and module
interface types that sit between per-file fact construction and matching or
project linking. The model keeps provider-neutral identities opaque, shares
lifecycle evidence between local and qualified flows, and preserves
deterministic compact evidence. The main readability risks are that important
invariants still live in repeated consumer branches or unchecked indices, and
that public-within-analysis constructors expose ownership boundaries that the
types do not express.

The earlier reports covering stream storage, effect-builder ownership,
projector state, and module linking were cross-checked. This report focuses on
the retained model APIs themselves and does not repeat those findings.

## Findings

### Fact construction and argument semantics

#### [x] READ-048 — Make the fact stream the sole owner of fact identity

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/fact.rs:481-502`; `analysis/facts/stream.rs:194-230`; consumers under `analysis/facts`, `analysis/flow`, and `analysis/matching`

`SemanticFact::new` is callable throughout `crate::analysis` and accepts an
arbitrary `FactId`, span, function owner, and payload. In production,
`FactStream::try_push` is the component that assigns the next artifact-local
ID, enforces `MAX_FACTS`, and stops appending after an invalid or exhausted
stream. The retained fact type does not encode that construction boundary, so
another analysis subsystem can bypass the stream's sequence and budget checks
while still producing a structurally valid `SemanticFact`. The test-only
`FactStream::push` performs a separate sequence check, which further splits the
creation contract between the model and its storage owner.

This makes the ID and trust-state invariants dependent on convention. A future
producer that constructs facts directly could create duplicate or out-of-order
artifact-local identities, and downstream indexes would have no single place
to reject the violation.

**Recommendation:** Move production fact construction behind a narrow
`FactStream` operation that assigns the ID and retains the budget/validity
checks; make the general `SemanticFact` constructor private to that owner, or
replace it with a private model constructor plus narrowly scoped test fixtures.
Expose read-only accessors for fields currently read by downstream analysis as
needed, rather than widening construction visibility. Preserve artifact-local
IDs, source spans, function ownership, the `MAX_FACTS` cap, deterministic
append order, and the current fail-closed invalid-stream behavior.

**Fix Applied:** Added an unforgeable `FactStreamToken` owned by the
building stream. `SemanticFact` construction now requires that authority,
while production IDs are still assigned only by `FactStream::append`;
test fixtures use a test-only token. This preserves dense artifact-local
IDs, budget enforcement, and fail-closed stream behavior.

#### [x] READ-049 — Give retained calls one canonical effective-argument view

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** SIMPLIFY
- **Category:** Duplication
- **Location:** `glass-lint-core/src/analysis/model/fact.rs:294-403`; `analysis/flow/effect/mod.rs:265-274,409-423`; `analysis/matching/arguments/evaluator.rs:279-287`; wrapper construction in `analysis/facts/calls/wrapper.rs`

`FactPayload::Call` retains `args` and an optional `CallUnwrap`, whose
`effective_args` replaces the ordinary list for wrapper calls such as
`.call` and `.apply`. The retained model does not own the selection rule:
`CallEffectRef::effective_args` implements it once, the effect builder repeats
the `map_or` branch, and constrained argument matching repeats it again. Other
flow consumers use the effect reference, while matching reads the raw fact
shape. The same semantic choice—receiver/bound-argument projection versus
authored argument evidence—is therefore distributed across effect and matcher
code.

Adding another wrapper or argument consumer requires remembering which list is
canonical and whether missing, spread, or unknown arguments must remain
fail-closed. The current duplication can let a new consumer observe original
arguments while the flow path observes effective arguments, producing
inconsistent sink or constraint results.

**Recommendation:** Give the retained call model one canonical operation or a
dedicated `CallArgumentView` that selects effective arguments and separately
exposes authored argument spans/evidence when needed. Have
`CallEffectRef`, the effect builder, constrained matching, projection, cross
propagation, and summary sinks consume that operation, deleting their raw
`unwrap.map_or` branches. Preserve wrapper receiver removal, bound parameter
projection, `.call`/`.apply` handling, spread and unknown values, authored
source locations, and the current fail-closed behavior.

**Fix Applied:** Added `FactPayload::effective_call_args` as the retained-call
model’s canonical wrapper/effective-argument operation. The effect builder,
canonical call shape, and constrained evaluator now consume it, while
authored arguments remain available for identity and source-location logic.

### Flow limits and lifecycle evidence

#### [x] READ-050 — Make lifecycle index bounds a typed invariant

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:215-258,260-370,372-480`; lifecycle limit validation in `analysis/api/compiler/physical.rs:153-164` and `api/rule/query/limits.rs:9-10`

`IndexedEvidence` uses a 64-bit mask and documents a lifecycle key domain of
at most 64 entries. The public `RequirementIndex::new` and `SinkIndex::new`
constructors do not carry that bound, however. `insert` silently returns
`false` for an index at or above 64, `restore` ignores that result, and
`remove`/`remove_value` use `expect` after finding an entry. The compiler
currently validates the declaration counts, but the retained flow model also
accepts indices from internal callers independently of that plan validation.
The invariant is consequently split between compiler policy, an unvalidated
newtype, a silent admission failure, and a panic-backed removal assumption.

An invalid index can be mistaken for a duplicate or absent event, while a
future caller or malformed internal plan can make the evidence mask and
compiled lifecycle counts disagree. That is especially difficult to diagnose
because the generic `LifecycleEvidence` API returns ordinary booleans rather
than distinguishing duplicate, out-of-range, and accepted events.

**Recommendation:** Establish the 64-entry invariant at one boundary: use
validated index constructors or a bounded lifecycle-index type, and make
admission return a result that distinguishes duplicate from overflow while
marking the retained model incomplete. Keep
removal and rollback total for values admitted by that type, eliminating the
`expect` assumptions. Preserve the compact mask, deterministic sorted evidence,
the compiler's lifecycle limits, duplicate-event semantics, and fail-closed
unsupported/incomplete flow behavior. Keep local `FactId` and qualified event
domains distinct through the generic evidence owner.

**Fix Applied:** Made `RequirementIndex::new` and `SinkIndex::new` validated
constructors that reject indices outside the 64-entry evidence mask domain.
Planning callers now fail closed when construction is unavailable, while
indexed evidence no longer needs overflow checks or panic-backed removal
assumptions. Existing duplicate-event semantics and compact deterministic
storage are unchanged.

#### [x] READ-051 — Give operation budgets one explicit scope owner

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** SIMPLIFY
- **Category:** Complexity
- **Location:** `glass-lint-core/src/analysis/model/flow.rs:13-130`; consumers in `analysis/flow/cross/mod.rs:246-253`, `analysis/flow/projector/mod.rs:356-367`, and `analysis/project/projection.rs:443-446`

`FlowLimits` stores both `operations` and `local_operations`, but
`from_flow_operations` always initializes them to the same value and the test
constructor does the same. The comment explains a scope distinction—one
budget per local module versus one project-wide cross-file budget—but the
numeric policy is duplicated in the model rather than represented by the
budget owner. The cross-flow entry point also reconstructs `FlowLimits` twice
just to extract the project operation limit, while local projection receives a
full limits object and selects the parallel field.

The result is two names for one configured limit plus multiple places that
decide which scope consumes it. A future change to scale or cap local and
project work differently can update one field, constructor, or call site and
silently alter only one flow phase.

**Recommendation:** Keep the shared resource limits in `FlowLimits`, but move
scope-specific operation budgets to explicit factories or typed owners such as
`LocalFlowBudget` and `ProjectFlowBudget`. If the limits remain a single
configuration value, remove the duplicate field and have the local projector
and cross phase request their scoped budget from one operation. Preserve the
per-module versus project-wide accounting semantics, exhaustion reporting, and
all other scaled limits.

**Fix Applied:** Removed the duplicated local-operation field from
`FlowLimits`; local and cross-flow phases now derive their independently
scoped `Budget` instances from one operation-limit owner, with cross-flow
setup computing that limit once. Verified with `make fmt && make ci`.

### Module request ownership

#### [x] READ-052 — Validate module request ownership when recording star exports

- **Severity:** Medium
- **Fix Complexity:** Low
- **Theme:** ENCAPSULATE
- **Category:** API
- **Location:** `glass-lint-core/src/analysis/model/module.rs:118-135,192-212,275-297`; consumers in `analysis/project/resolver.rs:141-178` and `analysis/project/identities.rs:197-203`

`ModuleRequestId` is an index into one `ModuleInterface`'s request vector,
but its public constructor is intentionally absent and `add_request` returns
the ID for the current interface. `ModuleInterface::add_star_export` still
accepts any `ModuleRequestId` and appends it without checking that the ID
belongs to this interface or denotes a request with the expected star-export
role. The normal interface builder passes a freshly returned local ID, but the
retained model does not enforce that relationship.

Downstream resolver and identity code must recover from an invalid ID by
calling `request` and treating `None` as an unknown star export. That silently
drops a re-export edge and changes resolution from a concrete candidate to an
unknown/ambiguous result; it also leaves the invalid ID in the retained state
until each consumer filters it.

**Recommendation:** Make star-export admission accept only an opaque request
handle minted by the owning interface, validate its role, and return a typed
rejection before storage. The interface builder can then mark the module
incomplete/unknown at the source. Preserve request order, repeated-request
behavior if it is meaningful, unknown-export clearing, cross-module resolution,
and fail-closed handling for genuinely unresolved requests.

**Fix Applied:** Replaced arbitrary star-export ID admission with an owning
`add_star_export_request` transaction that mints the request and records it
in the same interface. This removes the invalid cross-interface append path
while preserving fail-closed unknown-export behavior. Verified with
`make fmt && make ci`.

## Systemic Themes

- Retained model types contain the right provider-neutral identities, but
  several invariants remain encoded in caller conventions: fact sequencing,
  effective argument selection, lifecycle index bounds, and module-request
  ownership.
- Compact bounded representations are useful for deterministic analysis, but
  their bounds should be enforced at the domain boundary rather than inferred
  from compiler validation and later `bool`/`expect` behavior.
- Scope-specific accounting should be represented by the owner of the budget
  lifecycle. A shared configuration may describe resource limits, but it
  should not expose parallel fields whose current values are always equal.
- Flow and module changes must continue to preserve strict artifact-local
  identity, deterministic evidence order, and fail-closed unknown or
  unsupported alternatives.

## Decisions

- Tests should use a dedicated stream fixture that assigns IDs and applies the
  same budget boundary as production. Direct `SemanticFact` construction is
  not a supported test shortcut; private model construction may remain local
  to the fixture.
- Lifecycle indexes are validated at construction, while runtime admission
  returns a typed overflow/incomplete outcome shared by local and qualified
  evidence. Overflow is not an impossible compiler bug because retained model
  consumers still need fail-closed behavior for malformed internal inputs.
- `ModuleRequestId` values are minted by their owning `ModuleInterface`, and
  star-export admission validates the handle and role, returning a typed
  rejection before storage. Invalid requests are therefore not retained for
  downstream consumers to rediscover.

## Coverage

Reviewed all types listed in Chunk 11 of `CODEBASE_STRUCTURE_CORE.md`:

- Fact model: `ArgumentView`, `Building`, `CallArgInfo`, `CallUnwrap`,
  `ClassFactRole`, `ControlKind`, `ControlRegionId`, `FactId`, `FactPayload`,
  `Frozen`, `FunctionBoundary`, `ParameterBinding`, and `SemanticFact`.
- Flow model: `EvidenceValues`, `FlowId`, `FlowLimits`, `FlowState`,
  `FlowStateKey`, `EvidenceIndex`, `FunctionTable`, `IndexedEvidence`,
  `LifecycleEvidence`, `LifecycleRollback`, `RequirementIndex`, and
  `SinkIndex`.
- Module model: `ExportEntry`, `ImportedBinding`, `ModuleExport`,
  `ModuleInterface`, `ModuleRequest`, `ModuleRequestId`, `ModuleRequestRole`,
  and `ReExportBinding`.

The earlier Chunk 2, Chunk 3, Chunk 4, Chunk 7, Chunk 8, and Chunk 10 reports
were cross-checked to avoid repeating their stream lifecycle, effect-builder,
project context, obsolete module re-export, projector-state, and matcher-index
findings. No source, test, configuration, or existing audit files were
changed.
