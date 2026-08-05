# Codebase Readability Audit — Chunk 7

## Summary

Chunk 7 covers retained fact-stream types and the cross-file flow state,
source indexes, qualified call graph, worklists, propagation contexts, and
evidence containers. The fact stream uses a useful `Building`/`Frozen` phase
marker, and cross-flow keeps source-less reaching alternatives explicit so
certainty is not upgraded from incomplete evidence. The main risks are
correlated state represented as independent fields or caller-supplied values:
the fact freeze can attach unrelated name/value arenas, call contexts encode
mutually exclusive modes as two `Option`s, and bounded worklists infer
exhaustion from counts rather than reporting the admission result.

The highest-value changes are to make the fact freeze consume an owned table
bundle, represent cross-context origin as a sum type, and give context/source
worklists typed insertion outcomes. These changes should preserve the existing
bounded fixed points, deterministic ordering, and independent possible versus
definite witnesses.

No source, test, configuration, dependency, or documentation changes were made
by this audit.

## Findings

### Fact-stream phase boundary

#### [x] READ-037 — Bind frozen fact tables to their producing artifact

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture / identity ownership
- **Location:** `glass-lint-core/src/analysis/facts/stream.rs:287-304`,
  `analysis/resolution/mod.rs:167-174`,
  `analysis/facts/mod.rs:346-391`

`FactStream<Building>::freeze` accepts an arbitrary `NameTable` and
`ValueTable`, even though every `NameId` and `ValueId` in the facts and paths
belongs to the resolver that produced the building stream. The normal caller
passes `Resolver::freeze_into`’s tables, but the stream API does not express or
validate that pairing. A future internal caller can attach a different table
with the same artifact-local IDs and silently reinterpret member names,
static values, bindings, or call identities while the phase marker still says
the stream is valid.

Introduce a private frozen-table bundle or a consuming resolver/artifact
transition that owns the `NameTable`/`ValueTable` pairing, and delete the raw
two-table freeze entry point after migration. Preserve the `Building` to
`Frozen` type transition, dense fact/path identities, invalid-stream retention
for diagnostics, deterministic indexes, and the guarantee that names and
values are from the same local artifact.

**Fix Applied:** Added a resolver-owned `FrozenFactTables` bundle and changed
the building-to-frozen stream transition to consume that bundle instead of
independent name and value tables. The production constructor is private to
the resolver module; isolated stream tests use an explicit test-only factory.

**Verification:** `make fmt && make ci` passes, including all workspace tests,
end-to-end and provider rule harnesses, doctests, generated-rule validation,
and examples.

### Cross-flow context and worklist state

#### [ ] READ-038 — Represent call-context origin as an explicit sum type

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** Newtype / API / certainty state
- **Location:** `glass-lint-core/src/analysis/flow/cross/state.rs:112-204`,
  `analysis/flow/cross/mod.rs:60-104`,
  `analysis/flow/cross/propagation.rs:221-244`

`CallContext` encodes two mutually exclusive projection modes with
`parameter: Option<usize>` and `source_root: Option<ValueId>`. The production
constructors populate one field and clear the other, while
`matches_parameter` and `matches_source_root` rely on the implicit invariant
that the fields are never both set. The test constructor can create neither,
and adding another context mode would add another optional field plus more
implicit matching rules. `ContextProjection` and `CallPropagation` then
branch on those options while also carrying the cross-flow state and crossed
flag.

Replace the paired options with a private context-origin enum such as
`SourceRoot`, `TargetParameter`, or an explicit unknown origin, and make the
matching methods operate on that enum. Delete the mutually exclusive fields
and constructor protocol after migration. Preserve source-less reaching
alternatives, parameter-root requirements, source-root identity, crossed
versus local propagation, context hashing/deduplication, and the rule that
unknown input can downgrade certainty but cannot create evidence.

**Fix Applied:** None so far.

#### [ ] READ-039 — Return typed admission outcomes from bounded worklists

- **Severity:** High
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Bounded state / stop protocol
- **Location:** `glass-lint-core/src/analysis/flow/cross/worklist.rs:42-74`,
  `analysis/flow/cross/mod.rs:106-158`,
  `analysis/flow/cross/sources.rs:270-315`

`ContextWorklist::push` returns `false` for both a duplicate context and a
full retained set. Callers mostly ignore that result and infer exhaustion
from `len() >= max_retained`; `CrossWorklist::run` consequently returns
`ContextLimit` as soon as the retained count reaches the limit, even when the
queue has just drained and no candidate was dropped. `FlowSources::propagate`
has the same caller protocol: it checks `pending_seen.len()` before attempting
the set insertion and returns exhaustion without exposing whether a new item
was actually rejected. The bound, deduplication, and stop reason are spread
across the container and orchestration loops.

Give worklist insertion a typed result such as `Inserted`, `Duplicate`, or
`Full`, and let each owner record the stop reason only for an actual rejected
new item. Move frontier/seen-set admission and the corresponding budget
transition behind `ContextWorklist`/`FlowSources` operations; delete count-based
stop inference after migration. Preserve FIFO order, B-tree deterministic
deduplication, total-retained versus pending bounds, monotone candidate
propagation, source-less alternatives, and conservative incomplete outcomes
when a genuinely new context or candidate is rejected.

**Fix Applied:** None so far.

### Cross-flow state ownership

#### [ ] READ-040 — Make cross-flow evidence transitions atomic at the state owner

- **Severity:** Medium
- **Fix Complexity:** Medium
- **Theme:** ENCAPSULATE
- **Category:** API / Architecture / certainty state
- **Location:** `glass-lint-core/src/analysis/flow/cross/state.rs:57-109`,
  `analysis/flow/cross/propagation.rs:47-190`,
  `analysis/flow/cross/evidence.rs:105-210`

`CrossFlowState` delegates individual requirement and sink writes to
`LifecycleEvidence`, returns booleans, and exposes separate readiness checks.
`UsageProjector` clones the state, records a requirement or sink, emits based
on a later combination of `requirements_ready`, `source().is_some()`,
`sinks_complete`, and `crossed`, then assigns the clone back. The transition
that says “this use advances the state and is now eligible for emission” is
therefore assembled by the propagation caller rather than owned by the state
that stores the evidence. A new evidence kind or completion rule must update
the mutator, readiness checks, and every emission branch consistently.

Add narrow state-owner operations for requirement/sink advancement and
completion classification, returning a typed transition such as
`Advanced`, `AlreadyRecorded`, or `Ready`. Let propagation supply only the
matching event and emission sink, then delete the repeated readiness
combination branches. Preserve cloned branch state, source-less alternatives,
requirement/sink index semantics, prior-sink evidence, crossed-only emission,
and possible/definite certainty handling.

**Fix Applied:** None so far.

## Systemic Themes

Chunk 7 repeatedly uses private structs but leaves their most important
relationships implicit: fact IDs with their name/value tables, call-context
mode with optional fields, worklist capacity with insertion outcome, and
evidence mutation with readiness/emission policy. These relationships are
central to artifact-local identity and bounded certainty, so they should be
represented by owned transitions or sum types rather than comments and caller
discipline.

The existing phase marker, B-tree ordering, explicit source-less state, and
bounded worklist design are good foundations. Refactors should strengthen
those owners without merging local fact storage with project overlays or
turning incomplete alternatives into witnesses.

Search signals used for this chunk included phase transitions accepting raw
correlated tables, mutually exclusive `Option` fields, boolean admission
results, count-based exhaustion inference, and readiness/emission decisions
assembled across state and propagation modules.

## Open Questions

- The fact-freeze bundle should remain private and artifact-local; public
  callers should continue to receive only immutable `SemanticFacts` rather
  than table handles.
- Worklist limits distinguish total-retained memory from pending frontier
  memory; the typed admission result should preserve both metrics and their
  separate diagnostics.
- The next unreviewed handoff is Chunk 8: flow effects, planning, and
  projection types.

## Coverage

Reviewed the Chunk 7 types listed in `CODEBASE_STRUCTURE_CORE.md` across
fact-builder/fact-stream, call-result, call/interface/origin/state, and
cross-flow context, worklist, source, graph, propagation, state, and evidence
modules, with representative callers in lowering, resolution, local flow, and
report evidence. Existing Chunk 1–6 findings were checked to avoid re-reporting
fact traversal/pattern ownership, flow control/exhaustion policy, trace-chain
assembly, and project identity overlays. No findings are marked applied.
